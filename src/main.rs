use sa_placer_lib::*;

fn main() {
    let layout = build_simple_fpga_layout(64, 64);

    // let vis = layout.render_ascii();
    // std::fs::write("fpga_layout_vis.txt", vis).expect("Unable to write file");

    // let summary = layout.render_summary();
    // std::fs::write("fpga_layout_summary.txt", summary).expect("Unable to write file");

    let netlist: NetlistGraph = build_simple_netlist(300, 30, 100);
    println!("Netlist Summary: {:?}", netlist.count_summary());

    // let initial_solution = gen_random_placement(&layout, &netlist);
    let initial_solution = gen_random_placement(&layout, &netlist);
    std::fs::write(
        "fpga_layout_solution_initial.svg",
        initial_solution.render_svg(),
    )
    .expect("Unable to write file");

    println!("Starting SA Placer");
    let final_solution: PlacementSolution = fast_sa_placer(initial_solution, 500, 16, false);
    println!("SA Placer Complete");

    std::fs::write(
        "fpga_layout_solution_final.svg",
        final_solution.render_svg(),
    )
    .expect("Unable to write file");
}
