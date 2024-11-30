use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;
use viewer::algorithms::pathfinding::{do_pathfinding, PathSectionSettings};
use viewer::graph_storage::{load_binary, load_file};
use viewer::threading::NullStatusWriter;

fn criterion_benchmark(c: &mut Criterion) {
    println!("Loading");
    let res = load_file(&NullStatusWriter).unwrap();
    println!("Loaded; processing");
    let bin = load_binary(&NullStatusWriter, res).unwrap();

    println!("File processed");

    let viewer = &bin.viewer;
    let rng = &mut rand::thread_rng();
    c.bench_function("fib 20", |b| {
        b.iter(|| {
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

            std::hint::black_box(path);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
