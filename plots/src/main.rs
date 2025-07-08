use ahash::AHashMap;
use graph_format::{GraphFile, Readable};
use inline_python::python;
use rayon::prelude::*;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use std::ffi::CStr;

#[macro_export]
macro_rules! log
{
    ($($arg:tt)*) =>
    {
        {
            let now = std::time::Instant::now();
            println!("[{}] [{}:{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                file!(), line!(), format_args!($($arg)*));
        }
    }
}

fn main() {
    let file = GraphFile::read_from_file("graph_n4j.bin").unwrap();

    let avg_id = stats::mean(file.nodes.iter().map(|n| {
        unsafe { CStr::from_ptr(file.ids.as_ptr().add(n.offset_id as usize) as *const _) }
            .count_bytes()
    }));
    let avg_name = stats::mean(file.nodes.iter().map(|n| {
        unsafe { CStr::from_ptr(file.names.as_ptr().add(n.offset_name as usize) as *const _) }
            .count_bytes()
    }));

    println!("Average id length: {}", avg_id);
    println!("Average name length: {}", avg_name);

    let mut degrees = AHashMap::new();
    let persons = file.get_adjacency();

    let total_edges = persons.iter().map(|p| p.len()).sum::<usize>() / 2;
    println!("Total edges: {}", total_edges);

    let average_friend_count = 2.0 * total_edges as f64 / persons.len() as f64;

    const SCALE: f64 = 0.5;

    /*let mut res_map = HashMap::<usize, usize>::new();

    for freq in persons.iter().map(|p| {
        (p.iter().map(|&f| persons[f as usize].len()).sum::<usize>() as f64 / p.len() as f64) / p.len() as f64
    }) {
        let key = (freq * SCALE) as usize;
        *res_map.entry(key).or_default() += 1;
    }*/

    /*const SAMPLES: usize = 1000000;

    let ids = rand::thread_rng().sample_iter(rand::distributions::Uniform::new(0, persons.len())).take(SAMPLES).collect::<Vec<_>>();

    let samples = (0..SAMPLES)
        .into_par_iter()
        .map_init(|| rand::thread_rng(), |rng, _| {
            todo!();
        });*/

    /*let max_dist = |start_node| {
        let mut visited = vec![false; persons.len()];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((start_node as usize, 0));
        let mut max_dist = 0;
        while let Some((node, dist)) = queue.pop_front() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            max_dist = dist;
            for &neigh in &persons[node] {
                queue.push_back((neigh as usize, dist + 1));
            }
        }
        max_dist
    };
    log!("Getting bfs");
    let v = (0..100).into_par_iter().map(max_dist).max().unwrap();
    log!("Bfs from all nodes: {}", v);
    return;*/

    println!(
        "Average degree: {}",
        persons.iter().map(|p| p.len()).sum::<usize>() as f64 / persons.len() as f64
    );
    for pers in persons {
        match degrees.entry(pers.len()) {
            Occupied(mut e) => {
                *e.get_mut() += 1;
            }
            Vacant(e) => {
                e.insert(1);
            }
        }
    }
    let mut degrees_vec: Vec<_> = degrees.into_iter().collect();
    degrees_vec.sort_by_key(|(k, _)| *k);

    python! {
        import matplotlib.pyplot as plt
        import numpy as np
        import math
        from scipy.optimize import curve_fit

        def power(x, b, c):
            return b*x**c

        bin_data = 'res_map

        print(sorted(bin_data))

        plt.hist(list(bin_data.keys()), bins=100, weights=list(bin_data.values()), density=True)
        plt.show()

        plt.clf()

        mind, maxd = 6, 80
        dat = np.array('degrees_vec)[mind:maxd]
        deg_x = dat[:, 0]
        total_users = np.sum(dat[:, 1])
        deg_y = np.array(dat[:, 1], dtype=np.float64) / float(total_users)

        plt.title("Friend count distribution among {} Facebook users".format(total_users))

        # find power law
        popt, pcov = curve_fit(power, deg_x, deg_y, p0=[1, -1])
        print(popt)

        # plot log y as bar chart
        plt.bar(deg_x, deg_y)
        plt.xlabel("Friend count")
        plt.ylabel("User count")

        # plot power law
        x = np.linspace(mind, maxd, 100)
        plt.plot(x, power(x, *popt), "r-", label="fit: %5.3f*friends^%5.3f" % tuple(popt))

        plt.legend()

        # set y range to dat

        logarithmic = True

        if logarithmic:
            plt.xscale("log")
            plt.yscale("log")
            plt.ylim([1e-7, 1])
        else:
            plt.ylim([0, np.max(deg_y)])

        plt.show()
    }
}
