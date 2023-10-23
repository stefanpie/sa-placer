use sa_placer_lib::*;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn sa_placer_benchmark(c: &mut Criterion) {
    // let layout = build_simple_fpga_layout(64, 64);
    // let netlist = build_simple_netlist(300, 30, 100);
    // let initial_solution = gen_random_placement(layout, netlist);
    let layout = black_box(build_simple_fpga_layout(64, 64));
    let netlist: NetlistGraph = black_box(build_simple_netlist(300, 30, 100));
    let initial_solution = black_box(gen_random_placement(&layout, &netlist));

    c.bench_function("fast_sa_placer", |b| {
        b.iter(|| fast_sa_placer(initial_solution.clone(), 500, 16, false))
    });
}

fn sa_placer_large_benchmark(c: &mut Criterion) {
    let layout = build_simple_fpga_layout(200, 200);
    let netlist = build_simple_netlist(1000, 50, 200);
    let initial_solution = black_box(gen_random_placement(&layout, &netlist));

    c.bench_function("fast_sa_placer_large", |b| {
        b.iter(|| {
            fast_sa_placer(
                black_box(initial_solution.clone()),
                black_box(500),
                black_box(16),
                false,
            )
        })
    });
}

criterion_group!(benches, sa_placer_benchmark);
criterion_main!(benches);
