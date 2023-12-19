use std::{collections::HashMap, process::Command, sync::Mutex};

use rayon::prelude::*;
use sa_placer_lib::*;

fn render_solution_to_png(
    solution: &PlacementSolution<'_>,
    output_name: &str,
    output_dir: &str,
    keep_svg: bool,
) {
    std::fs::write(
        // format!("fpga_layout_solution_{}.svg", solution_name),
        format!("{}/{}.svg", output_dir, output_name),
        solution.render_svg(),
    )
    .expect("Unable to write file");
    let _output = Command::new("magick")
        .arg("convert")
        .arg("-size")
        .arg("800x800")
        .arg(format!("{}/{}.svg", output_dir, output_name))
        .arg(format!("{}/{}.png", output_dir, output_name))
        .output()
        .expect("failed to execute process");
    if !keep_svg {
        std::fs::remove_file(format!("{}/{}.svg", output_dir, output_name))
            .expect("Unable to remove file");
    }
}

fn main() {
    // limit the number of threads to 6 becasue my laptop has 12 logical cores
    rayon::ThreadPoolBuilder::new()
        .num_threads(6)
        .build_global()
        .unwrap();

    // make a data directory
    if std::path::Path::new("./output_data").exists() {
        std::fs::remove_dir_all("./output_data").expect("Unable to remove directory");
    }
    std::fs::create_dir_all("./output_data").expect("Unable to create directory");

    // build FPGA layout
    let layout = build_simple_fpga_layout(64, 64);

    // ascii render of the layout
    let vis = layout.render_ascii();
    std::fs::write("./output_data/fpga_layout.txt", vis).expect("Unable to write file");

    // text summary of the fpga layout
    let summary = layout.render_summary();
    std::fs::write("./output_data/fpga_layout_summary.txt", summary).expect("Unable to write file");

    // build a random netlist
    let netlist: NetlistGraph = build_simple_netlist(300, 30, 100);

    // build a random initial placement solution
    let inital_placement_method = InitialPlacerMethod::Random;
    let initial_solution = gen_initial_placement(&layout, &netlist, inital_placement_method);

    render_solution_to_png(
        &initial_solution,
        "initial_solution",
        "./output_data",
        false,
    );

    // values of n_neighbors to explore
    let configs_n_neighbors = vec![1, 2, 4, 8, 16, 32, 64, 128, 256];

    // number of steps to run for all placement runs
    let n_steps = 1000;

    let x_data_collection = Mutex::new(HashMap::new());
    let y_data_collection = Mutex::new(HashMap::new());
    let render_collection = Mutex::new(HashMap::new());
    let final_solution_collection = Mutex::new(HashMap::new());
    let config_data_collection = configs_n_neighbors.clone();

    config_data_collection.par_iter().for_each(|&n_neighbors| {
        println!("Running SA Placer with {} neighbors", n_neighbors);
        let placer_output =
            fast_sa_placer(initial_solution.clone(), n_steps, n_neighbors, false, true);

        let final_solution: PlacementSolution<'_> = placer_output.final_solution;

        // write the x and y data to csv
        let x_data: Vec<u32> = placer_output.x_steps;
        let y_data = placer_output.y_cost;

        let mut x_data_collection = x_data_collection.lock().unwrap();
        let mut y_data_collection = y_data_collection.lock().unwrap();
        x_data_collection.insert(n_neighbors, x_data);
        y_data_collection.insert(n_neighbors, y_data);

        let mut render_collection = render_collection.lock().unwrap();
        let mut final_solution_collection = final_solution_collection.lock().unwrap();
        render_collection.insert(n_neighbors, placer_output.renderer.unwrap());
        final_solution_collection.insert(n_neighbors, final_solution.clone());
    });

    // remove the mutexes
    let x_data_collection = x_data_collection.into_inner().unwrap();
    let y_data_collection = y_data_collection.into_inner().unwrap();

    let final_solution_collection = final_solution_collection.into_inner().unwrap();
    let render_collection = render_collection.into_inner().unwrap();

    // csv for each n_neighbors
    for n in config_data_collection.clone().iter() {
        let mut wtr: csv::Writer<std::fs::File> =
            csv::Writer::from_path(format!("output_data/fpga_placer_history_{}.csv", n)).unwrap();
        wtr.write_record(&["step", "obj_fn_value"]).unwrap();
        let x_series = x_data_collection.get(n).unwrap();
        let y_series = y_data_collection.get(n).unwrap();
        for (x, y) in x_series.iter().zip(y_series.iter()) {
            wtr.write_record(&[x.to_string(), y.to_string()]).unwrap();
        }
        wtr.flush().unwrap();
    }

    // one big csv with all the data
    let mut wtr: csv::Writer<std::fs::File> =
        csv::Writer::from_path("output_data/fpga_placer_history.csv").unwrap();
    wtr.write_record(&["step", "obj_fn_value", "n_neighbors"])
        .unwrap();
    for n_neighbors in config_data_collection.clone() {
        let x_series = x_data_collection.get(&n_neighbors).unwrap();
        let y_series = y_data_collection.get(&n_neighbors).unwrap();
        for (x, y) in x_series.iter().zip(y_series.iter()) {
            wtr.write_record(&[x.to_string(), y.to_string(), n_neighbors.to_string()])
                .unwrap();
        }
    }

    let mut gnuplot_command = String::new();
    gnuplot_command.push_str("set terminal png size 1000,500; ");
    gnuplot_command.push_str("set output 'output_data/fpga_placer_history.png'; ");
    gnuplot_command.push_str("set datafile separator ','; ");
    gnuplot_command.push_str("set title 'FPGA Placement History'; ");
    gnuplot_command.push_str("set xlabel 'Step'; ");
    gnuplot_command.push_str("set ylabel 'Objective Function Value'; ");
    gnuplot_command.push_str("set yrange [0:*]; ");
    gnuplot_command.push_str("plot ");
    for n_neighbors in config_data_collection.clone() {
        gnuplot_command.push_str(
            format!(
                "'output_data/fpga_placer_history_{}.csv' using 1:2 title '{} neighbors' with lines, ",
                n_neighbors, n_neighbors
            )
            .as_str(),
        );
    }
    gnuplot_command.pop();
    gnuplot_command.pop();
    gnuplot_command.push(';');

    let _output = Command::new("gnuplot")
        .arg("-p")
        .arg("-e")
        .arg(&gnuplot_command)
        .output()
        .expect("failed to execute process");

    // render_solution_to_png(&selected_final_solution, "fpga_final_solution");

    // selected_renderer.render_to_video("./placer_animation", 30.0, 20);
    config_data_collection.par_iter().for_each(|&n_neighbors| {
        let solution = final_solution_collection.get(&n_neighbors).unwrap();
        render_solution_to_png(
            &solution,
            &format!("final_solution_{}", n_neighbors),
            "./output_data",
            false,
        );

        let renderer: Renderer = render_collection.get(&n_neighbors).unwrap().clone();
        renderer.render_to_video(
            &format!("placer_animation_{}", n_neighbors),
            "./output_data",
            30.0,
            20,
            true,
        );
    });
}
