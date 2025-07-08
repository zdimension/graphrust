#![feature(const_float_round_methods)]
#![feature(coroutines)]
#![feature(iter_from_coroutine)]

use ahash::HashSet;
use env_logger;
use futures_util::TryStreamExt;
use graph_format::nalgebra::{Vector, U10, U13, U15};
use inline_python::python;
use itertools::Itertools;
use neo4rs::{query, ConfigBuilder, Graph};
use rand::Rng;
use std::fmt::Debug;
use std::num::NonZeroU16;
use std::pin::pin;
use std::sync::Arc;
use std::{env, iter, thread};
use viewer::algorithms::pathfinding::{do_pathfinding, PathSectionSettings};
use viewer::graph_storage::{load_binary, load_file};
use viewer::threading::NullStatusWriter;

#[test]
fn init_logs() {
    unsafe {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::builder()
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{}] [{}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
}

fn find_fixed_point<State: Copy + Debug, Value>(
    precision: f64,
    min_max: (Option<usize>, Option<usize>),
    mut iter: impl Iterator<Item = Value>,
    init: State,
    mut dist: impl Fn(State, State) -> f64,
    mut step: impl FnMut(usize, State, Value) -> State,
) -> (usize, Option<State>) {
    let mut state = init;
    let (min, max) = min_max;
    let (min, max) = (min.unwrap_or(1), max.unwrap_or(usize::MAX));
    let mut n = 0;

    for value in iter {
        n += 1;
        let new_state = step(n, state, value);
        let current_dist = dist(state, new_state);

        if n > min && current_dist < precision {
            return (n, Some(new_state));
        }

        if n >= max {
            log::info!(
                "Reached maximum iterations ({}) without convergence,\
                state={:?}, new_state={:?}, dist={}",
                max,
                state,
                new_state,
                current_dist
            );
            break;
        }

        state = new_state;
    }

    (n, None)
}

#[tokio::test]
async fn it_works() {
    // print the current directory
    //println!("Current directory: {:?}", env::current_dir().unwrap());
    log::info!("Loading");
    let res = load_file(&NullStatusWriter).unwrap();
    log::info!("Loaded; processing");
    let bin = load_binary(&NullStatusWriter, res).unwrap();

    log::info!("File processed");

    let viewer = &bin.viewer;
    let rng = &mut rand::thread_rng();

    const PRECISION: f64 = 0.0001;
    const DIGITS: usize = {
        let mut digits = 0;
        let mut precision = PRECISION;
        while precision < 1.0 {
            precision *= 10.0;
            digits += 1;
        }
        digits
    };

    let get_path_lens = || {
        #[coroutine]
        || loop {
            let rng = &mut rand::thread_rng();
            let node1 = rng.gen_range(0..viewer.persons.len());
            let node2 = rng.gen_range(0..viewer.persons.len());

            if node1 == node2 {
                continue; // skip if both nodes are the same
            }

            let path = do_pathfinding(
                PathSectionSettings {
                    path_src: Some(node1),
                    path_dest: Some(node2),
                    exclude_ids: vec![],
                    path_no_direct: false,
                    path_no_mutual: false,
                },
                &viewer.persons,
            );

            let Some(path) = path else {
                log::error!("Pathfinding failed for nodes {} and {}", node1, node2);
                continue;
            };

            let path_len = path.path.len();

            yield path_len;
        }
    };

    let get_path_lens_n4j = async || {
        use std::fs::OpenOptions;
        use std::io::Write;

        let n4j_config = ConfigBuilder::default()
            .uri("127.0.0.1:7687")
            .user("neo4j")
            .password("password")
            .fetch_size(10485760)
            .build()
            .unwrap();

        let graph = Graph::connect(n4j_config).await.unwrap();

        let total_node_count_n4j: usize = graph
            .execute(query("match (n) return count(n) as count"))
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get("count")
            .unwrap();

        let graph = Arc::new(graph);

        #[coroutine]
        static move || {
            if let Ok(file) = OpenOptions::new().read(true).open("distances.txt") {
                log::info!("Reading existing path lengths from distances.txt");
                use std::io::BufRead;
                let file = std::io::BufReader::new(file);
                for line in file.lines() {
                    let line = line.unwrap();
                    if let Ok(len) = line.parse::<usize>() {
                        yield len;
                    } else {
                        log::warn!("Failed to parse line: {}", line);
                    }
                }
                return;
            }

            loop {
                let rng = &mut rand::thread_rng();

                const BATCH_SIZE: usize = 50;

                let node_ids = (0..(2 * BATCH_SIZE))
                    .map(|_| rng.gen_range(0..total_node_count_n4j as i64))
                    .collect_vec();

                //log::info!("Pairs: {:?}", node_ids);

                //let graph = Arc::clone(&graph);
                let lengths = thread::spawn(move || {
                    tokio::runtime::Runtime::new().unwrap().block_on(async {
                        let n4j_config = ConfigBuilder::default()
                            .uri("127.0.0.1:7687")
                            .user("neo4j")
                            .password("password")
                            .fetch_size(10485760)
                            .build()
                            .unwrap();

                        let graph = Graph::connect(n4j_config).await.unwrap();

                        log::info!("Fetching path lengths for {} pairs", BATCH_SIZE);
                        let res = graph
                            .execute(
                                query(
                                    "
WITH $ids AS idList
WITH [i IN range(0, size(idList)-2, 2) | [idList[i], idList[i+1]]] AS pairs
UNWIND pairs AS pair
MATCH (a), (b)
WHERE id(a) = pair[0] AND id(b) = pair[1]
MATCH path = shortestPath((a)-[*]-(b))
RETURN a.uid AS from, b.uid AS to, 
       CASE WHEN path IS NOT NULL THEN length(path) ELSE null END AS distance",
                                )
                                .param("ids", node_ids),
                            )
                            .await
                            .unwrap()
                            .column_into_stream::<i64>("distance")
                            .try_collect::<Vec<_>>()
                            .await
                            .unwrap();
                        log::info!("Fetched path lengths for {} pairs", BATCH_SIZE);

                        let mut file = OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open("distances.txt")
                            .unwrap();
                        for &len in &res {
                            writeln!(file, "{}", len).unwrap();
                        }

                        res
                    })
                })
                .join()
                .unwrap();

                for length in lengths {
                    yield length as usize;
                }
            }
        }
    };

    let dist_hist = find_fixed_point(
        f64::INFINITY,
        (Some(2000), None),
        iter::from_coroutine(get_path_lens()),
        Vector::<f64, U13, _>::zeros(),
        |a, b| (a - b).norm(),
        |_, mut acc, len| {
            if len < acc.len() {
                acc[len] += 1.0;
            } else {
                log::warn!("Path length {} exceeds histogram size {}", len, acc.len());
            }
            acc
        },
    );

    log::info!(
        "Distance histogram stabilized to {:?} after {} samples",
        dist_hist.1,
        dist_hist.0
    );

    let dist_hist_n4j = find_fixed_point(
        f64::INFINITY,
        (Some(20000), None),
        iter::from_coroutine(pin!(get_path_lens_n4j().await)),
        Vector::<f64, U15, _>::zeros(),
        |a, b| (a - b).norm(),
        |_, mut acc, len| {
            if len < acc.len() {
                acc[len] += 1.0;
            } else {
                log::warn!("Path length {} exceeds histogram size {}", len, acc.len());
            }
            acc
        },
    );

    log::info!(
        "Distance histogram (N4J) stabilized to {:?} after {} samples",
        dist_hist_n4j.1,
        dist_hist_n4j.0
    );

    let mut avg_distances = Vec::new();
    let dist_avg = find_fixed_point(
        PRECISION,
        (Some(20), None),
        iter::from_coroutine(get_path_lens()),
        (0.0f64, 0.0f64),
        |(_, old_avg), (_, new_avg)| (old_avg - new_avg).abs(),
        |i, (acc, old_avg), dist: usize| {
            let new_acc = acc + dist as f64;
            let new_avg = new_acc / i as f64;

            avg_distances.push(new_avg);

            (new_acc, new_avg)
        },
    );

    let final_avg = dist_avg.1.map_or(f64::NAN, |(_, avg)| avg);

    log::info!(
        "Average path length stabilized to {:.DIGITS$} after {} samples",
        final_avg,
        dist_avg.0
    );

    let mut avg_distances_n4j = Vec::new();
    let dist_avg_n4j = find_fixed_point(
        PRECISION,
        (Some(20), None),
        iter::from_coroutine(pin!(get_path_lens_n4j().await)),
        (0.0f64, 0.0f64),
        |(_, old_avg), (_, new_avg)| (old_avg - new_avg).abs(),
        |i, (acc, old_avg), dist: usize| {
            let new_acc = acc + dist as f64;
            let new_avg = new_acc / i as f64;

            avg_distances_n4j.push(new_avg);

            (new_acc, new_avg)
        },
    );

    let final_avg_n4j = dist_avg_n4j.1.map_or(f64::NAN, |(_, avg)| avg);

    log::info!(
        "Average path length (N4J) stabilized to {:.DIGITS$} after {} samples",
        final_avg_n4j,
        dist_avg_n4j.0
    );

    /*let mut hist = Vector::<f64, U10, _>::zeros();

    let mut final_avg = -1000.0;
    let mut final_hist = hist;

    loop {
        let node1 = rng.gen_range(0..viewer.persons.len());
        let node2 = rng.gen_range(0..viewer.persons.len());

        if node1 == node2 {
            continue; // skip if both nodes are the same
        }

        let path = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node1),
                path_dest: Some(node2),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        );

        let Some(path) = path else {
            log::error!("Pathfinding failed for nodes {} and {}", node1, node2);
            continue;
        };

        let path_len = path.path.len();

        let mut new_hist = hist;
        if path_len < new_hist.len() {
            new_hist[path_len] += 1.0;
        } else {
            log::warn!(
                "Path length {} exceeds histogram size {}",
                path_len,
                new_hist.len()
            );
        }

        let hist_dist = (new_hist - hist).norm();

        hist = new_hist;

        if final_avg != -1000.0 && num_samples > 20 && hist_dist < PRECISION {
            log::info!(
                "Histogram stabilized after {} samples with distance {}",
                num_samples,
                hist_dist
            );
            final_hist = new_hist;
            break;
        }

        if final_avg == -1000.0 {
            distances.push(path_len);

            distance += path_len;
            num_samples += 1;

            let new_avg = distance as f64 / num_samples as f64;
            let delta = (new_avg - prev_avg).abs();
            if num_samples > 15 && delta < PRECISION {
                log::info!(
                    "Average path length: {:.DIGITS$} ({} samples, distance {})",
                    new_avg,
                    num_samples,
                    distance
                );
                final_avg = new_avg;
            }

            prev_avg = new_avg;
            avg_distances.push(new_avg);
        }
    }*/

    let hist_list = dist_hist.1.unwrap().iter().copied().collect_vec();
    let hist_list_n4j = dist_hist_n4j.1.unwrap().iter().copied().collect_vec();

    python! {
        import matplotlib.pyplot as plt

        f = plt.figure(1)
        plt.plot('avg_distances, label="Average Path Length")
        plt.axhline(y='final_avg, color="red", linestyle="--", label="Final Average: " + str(round('final_avg, 'DIGITS)))
        plt.xlabel("Running sample count")
        plt.ylabel("Average Path Length")
        plt.title("Average Path Length Over Samples")
        plt.legend()
        f.show()

        f2 = plt.figure(4)
        plt.plot('avg_distances_n4j, label="Average Path Length (N4J)")
        plt.axhline(y='final_avg_n4j, color="red", linestyle="--", label="Final Average: " + str(round('final_avg_n4j, 'DIGITS)))
        plt.xlabel("Running sample count")
        plt.ylabel("Average Path Length")
        plt.title("Average Path Length Over Samples (N4J)")
        plt.legend()
        f2.show()

        # now plot the distributon of distances, binned by 1
        # distances are always integers, so we can use a histogram
        import numpy as np

        hist = 'hist_list
        bins = np.arange(len(hist) + 1)

        g = plt.figure(2)
        plt.bar(bins[:-1], hist, width=1, edgecolor="black")
        plt.xlabel("Path Length")
        plt.ylabel("Frequency")
        plt.title("Distribution of Path Lengths")
        g.show()

        hist = 'hist_list_n4j
        bins = np.arange(len(hist) + 1)

        h = plt.figure(3)
        plt.bar(bins[:-1], hist, width=1, edgecolor="black")
        plt.xlabel("Path Length")
        plt.ylabel("Frequency")
        plt.title("Distribution of Path Lengths (N4J)")
        h.show()

        input()
    }

    std::process::exit(0);

    return;

    let mut node = rng.gen_range(0..viewer.persons.len());
    let mut found_already = HashSet::default();
    for _ in 0..10 {
        // find furthest node using bfs
        let mut dist = vec![0; viewer.persons.len()];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(node);
        dist[node] = 1;
        while let Some(cur) = queue.pop_front() {
            for &neigh in viewer.persons[cur].neighbors {
                if dist[neigh] == 0 {
                    dist[neigh] = dist[cur] + 1;
                    queue.push_back(neigh);
                }
            }
        }
        /*let max_dist = dist.iter().max().unwrap();
        let furthest = dist
            .iter()
            .enumerate()
            .find(|(_, &d)| d == *max_dist && !found_already.contains(&d))
            .unwrap()
            .0;*/
        let furthest = dist
            .iter()
            .enumerate()
            .filter(|(i, _)| !found_already.contains(i))
            .max_by_key(|(_, &d)| d)
            .unwrap()
            .0;
        found_already.insert(furthest);
        let path = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node),
                path_dest: Some(furthest),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap()
        .path;
        log::info!(
            "diam = {} ({}); path [{}] : [{}]",
            dist[furthest],
            furthest,
            path.len(),
            path.iter()
                .map(|i| viewer.persons[*i].neighbors.len().to_string())
                .join(", ")
        );
        node = furthest;
    }

    /*for _ in 0..1000 {
        let node1 = rng.gen_range(0..viewer.persons.len());
        let node2 = rng.gen_range(0..viewer.persons.len());

        let path = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node1),
                path_dest: Some(node2),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap();

        let path2 = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node1),
                path_dest: Some(node2),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap();

        assert_eq!(path.path, path2.path);
    }*/

    /* let get = |name| {
        let r = viewer
            .engine
            .get_blocking(|s| s.search(name, 1)[0] as usize);
        println!("{}: {:?}", name, r);
        r
    };

    let swann = get("Benziane Swann");

    let craby = get("Craby Craby");

    let blaibiron = get("Charli BlaBiron");

    let etienne = get("Etienne Marais");

    let tom = get("Tom Niget");

    let path = do_pathfinding(
        PathSectionSettings {
            path_src: Some(swann),
            path_dest: Some(etienne),
            exclude_ids: vec![tom],
            path_no_direct: false,
            path_no_mutual: false,
        },
        &viewer.persons,
    )
    .unwrap();

    println!(
        "{:?}",
        path.path
            .iter()
            .map(|&id| &viewer.persons[id].name)
            .collect_vec()
    );*/
}
