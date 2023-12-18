use std::{process::Command, sync::Mutex};

use rayon::prelude::*;
use sa_placer_lib::*;

fn render_solution_to_png(solution: &PlacementSolution<'_>, solution_name: &str) {
    std::fs::write(
        format!("fpga_layout_solution_{}.svg", solution_name),
        solution.render_svg(),
    )
    .expect("Unable to write file");
    let _output = Command::new("magick")
        .arg("convert")
        .arg("-size")
        .arg("800x800")
        .arg(format!("fpga_layout_solution_{}.svg", solution_name))
        .arg(format!("fpga_layout_solution_{}.png", solution_name))
        .output()
        .expect("failed to execute process");
}

fn main() {
    let layout = build_simple_fpga_layout(64, 64);

    // let vis = layout.render_ascii();
    // std::fs::write("fpga_layout_vis.txt", vis).expect("Unable to write file");

    // let summary = layout.render_summary();
    // std::fs::write("fpga_layout_summary.txt", summary).expect("Unable to write file");

    let netlist: NetlistGraph = build_simple_netlist(300, 30, 100);

    let initial_solution = gen_random_placement(&layout, &netlist);

    render_solution_to_png(&initial_solution, "initial_solution");

    let configs_n_neighbors = vec![1, 2, 4, 8, 16, 32, 64, 128, 256];

    let n_steps = 500;

    let selected_final_n_neighbors = 16;
    let selected_final_solution: Mutex<Option<PlacementSolution>> = Mutex::new(None);
    let selected_renderer: Mutex<Option<Renderer>> = Mutex::new(None);

    let x_data_collection = Mutex::new(Vec::new());
    let y_data_collection = Mutex::new(Vec::new());
    let config_data_collection = configs_n_neighbors.clone();

    config_data_collection.par_iter().for_each(|&n_neighbors| {
        println!("Running SA Placer with {} neighbors", n_neighbors);
        let placer_output = match n_neighbors == selected_final_n_neighbors {
            true => fast_sa_placer(initial_solution.clone(), n_steps, n_neighbors, false, true),
            false => fast_sa_placer(initial_solution.clone(), n_steps, n_neighbors, false, false),
        };

        let final_solution = placer_output.final_solution;

        // write the x and y data to csv
        let x_data: Vec<u32> = placer_output.x_steps;
        let y_data = placer_output.y_cost;

        let mut x_data_collection = x_data_collection.lock().unwrap();
        let mut y_data_collection = y_data_collection.lock().unwrap();
        x_data_collection.push(x_data);
        y_data_collection.push(y_data);

        if n_neighbors == selected_final_n_neighbors {
            let mut selected_final_solution = selected_final_solution.lock().unwrap();
            *selected_final_solution = Some(final_solution.clone());

            let mut selected_renderer = selected_renderer.lock().unwrap();
            *selected_renderer = Some(placer_output.renderer.unwrap());
        }
    });

    // remove the mutexes
    let x_data_collection = x_data_collection.into_inner().unwrap();
    let y_data_collection = y_data_collection.into_inner().unwrap();

    let selected_final_solution = selected_final_solution.into_inner().unwrap().unwrap();
    let selected_renderer = selected_renderer.into_inner().unwrap().unwrap();

    std::fs::create_dir_all("output_data").expect("Unable to create directory");

    for ((x, y), n_neighbors) in x_data_collection
        .iter()
        .zip(y_data_collection.iter())
        .zip(config_data_collection.clone().iter())
    {
        let mut wtr = csv::Writer::from_path(format!(
            "output_data/fpga_placer_history_{}.csv",
            n_neighbors
        ))
        .unwrap();
        wtr.write_record(&["step", "obj_fn_value"]).unwrap();
        for (x, y) in x.iter().zip(y.iter()) {
            wtr.write_record(&[x.to_string(), y.to_string()]).unwrap();
        }
        wtr.flush().unwrap();
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
    println!("{}", gnuplot_command);

    let _output = Command::new("gnuplot")
        .arg("-p")
        .arg("-e")
        .arg(&gnuplot_command)
        .output()
        .expect("failed to execute process");

    render_solution_to_png(&selected_final_solution, "fpga_final_solution");

    selected_renderer.render_to_video("./placer_animation", 60.0)
}
