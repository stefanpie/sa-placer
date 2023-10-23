use std::error;
use std::process::Command;

use rand::seq::SliceRandom;
use rand::Rng;
use rayon::prelude::IntoParallelIterator;
use rayon::prelude::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use rustworkx_core::petgraph::visit::EdgeRef;
use tempfile::tempdir;

use super::fpga_layout::*;
use super::netlist::*;

use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

#[derive(Debug, Clone, Copy)]
pub enum PlacementAction {
    MOVE,
    SWAP,
    MOVE_DIRECTED,
}

#[derive(Debug, Clone)]
pub struct PlacementSolution<'a> {
    pub layout: &'a FPGALayout,
    pub netlist: &'a NetlistGraph,
    pub solution_map: FxHashMap<NetlistNode, FPGALayoutCoordinate>,
}

impl<'a> PlacementSolution<'a> {
    pub fn new(layout: &'a FPGALayout, netlist: &'a NetlistGraph) -> Self {
        Self {
            layout,
            netlist,
            solution_map: FxHashMap::default(),
        }
    }

    pub fn action_move(&mut self) {
        let mut rng = rand::thread_rng();

        // Randomly select a node
        let node = match self.netlist.all_nodes().choose(&mut rng) {
            Some(n) => n.clone(),
            None => return,
        };

        // Get possible sites
        let possible_sites = self.get_possible_sites(node.macro_type);

        // Return if there are no possible sites
        if possible_sites.is_empty() {
            return;
        }

        // Randomly select a location
        let location = match possible_sites.choose(&mut rng) {
            Some(l) => l.clone(),
            None => return,
        };

        self.solution_map.insert(node, location);
    }

    pub fn action_swap(&mut self) {
        let mut rng = rand::thread_rng();

        // Randomly select a node (node_a)
        let node_a = match self.netlist.all_nodes().choose(&mut rng) {
            Some(n) => n.clone(),
            None => return,
        };

        // Filter nodes of the same type as node_a
        let nodes_same_type: Vec<NetlistNode> = self
            .netlist
            .all_nodes()
            .iter()
            .filter(|&&node| node.macro_type == node_a.macro_type)
            .cloned()
            .collect();

        // If no nodes of the same type, return
        if nodes_same_type.is_empty() {
            return;
        }

        // Randomly select another node (node_b) of the same type
        let node_b = match nodes_same_type.choose(&mut rng) {
            Some(n) => n.clone(),
            None => return,
        };

        // Clone the locations first to avoid borrowing issues
        let loc_a = self.solution_map.get(&node_a).cloned();
        let loc_b = self.solution_map.get(&node_b).cloned();

        // Perform the swap
        if let (Some(loc_a), Some(loc_b)) = (loc_a, loc_b) {
            self.solution_map.insert(node_a, loc_b);
            self.solution_map.insert(node_b, loc_a);
        }
    }

    pub fn action_move_directed(&mut self) {
        let node_count = self.netlist.graph.node_count() as u32;

        if node_count == 0 {
            panic!("No nodes in netlist; cannot compute mean for MOVE_DIRECTED");
        }
        let x_mean = self
            .netlist
            .all_nodes()
            .iter()
            .map(|node| self.solution_map.get(node).unwrap().x)
            .sum::<u32>()
            / node_count;

        let y_mean = self
            .netlist
            .all_nodes()
            .iter()
            .map(|node| self.solution_map.get(node).unwrap().y)
            .sum::<u32>()
            / node_count;

        let mut rng = rand::thread_rng();

        // pick a random node
        let node: NetlistNode = self.netlist.all_nodes()[rng.gen_range(0..node_count as usize)];

        let valid_locations = self.get_possible_sites(node.macro_type);
        let valid_closest_location = valid_locations
            .iter()
            .min_by(|a, b| {
                let a_distance =
                    (a.x as i32 - x_mean as i32).abs() + (a.y as i32 - y_mean as i32).abs();
                let b_distance =
                    (b.x as i32 - x_mean as i32).abs() + (b.y as i32 - y_mean as i32).abs();
                a_distance.cmp(&b_distance)
            })
            .unwrap();

        // if the new location is futher away from the mean than the current location, return
        let current_location = self.solution_map.get(&node).unwrap();
        let current_distance = (current_location.x as i32 - x_mean as i32).abs()
            + (current_location.y as i32 - y_mean as i32).abs();
        let new_distance = (valid_closest_location.x as i32 - x_mean as i32).abs()
            + (valid_closest_location.y as i32 - y_mean as i32).abs();
        if new_distance > current_distance {
            return;
        }

        self.solution_map
            .insert(node, valid_closest_location.clone());
    }

    pub fn action(&mut self, action: PlacementAction) {
        match action {
            PlacementAction::MOVE => self.action_move(),
            PlacementAction::SWAP => self.action_swap(),
            PlacementAction::MOVE_DIRECTED => self.action_move_directed(),
        }
    }

    pub fn cost_bb(&self) -> f32 {
        let mut cost = 0.0;

        for edge in self.netlist.graph.edge_references() {
            let source_idx = edge.source();
            let target_idx = edge.target();

            let source = self.netlist.graph.node_weight(source_idx).unwrap();
            let target = self.netlist.graph.node_weight(target_idx).unwrap();

            let source_location = self.solution_map.get(source).unwrap();
            let target_location = self.solution_map.get(target).unwrap();

            let x_distance = (source_location.x as i32 - target_location.x as i32).abs();
            let y_distance = (source_location.y as i32 - target_location.y as i32).abs();

            let distance = x_distance + y_distance;
            cost += distance as f32;
        }

        cost
    }

    pub fn render_svg(&self) -> String {
        let mut svg = String::new();

        svg.push_str(&format!(
            "<svg width=\"{}\" height=\"{}\" xmlns=\"http://www.w3.org/2000/svg\" style=\"background-color:white\">\n",
            self.layout.width * 100,
            self.layout.height * 100
        ));

        // draw the white background manually
        svg.push_str(&format!(
            "\t<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\"/>\n",
            0,
            0,
            self.layout.width * 100,
            self.layout.height * 100
        ));

        // draw boxes for each location
        for x in 0..self.layout.width {
            for y in 0..self.layout.height {
                let layout_type = self.layout.get(&FPGALayoutCoordinate::new(x, y)).unwrap();

                let color = match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => "red",
                    FPGALayoutType::MacroType(MacroType::DSP) => "blue",
                    FPGALayoutType::MacroType(MacroType::BRAM) => "green",
                    FPGALayoutType::MacroType(MacroType::IO) => "yellow",
                    FPGALayoutType::EMPTY => "gray",
                };

                svg.push_str(&format!(
                    "\t<rect x=\"{}\" y=\"{}\" width=\"100\" height=\"100\" fill=\"{}\" fill-opacity=\"0.25\" stroke=\"black\" stroke-width=\"2\"/>\n",
                    x * 100,
                    y * 100,
                    color
                ));
            }
        }

        // draw boxes for each netlist node
        for (node, location) in self.solution_map.iter() {
            let color = match node.macro_type {
                MacroType::CLB => "red",
                MacroType::DSP => "blue",
                MacroType::BRAM => "green",
                MacroType::IO => "yellow",
            };

            svg.push_str(&format!(
                "\t<rect x=\"{}\" y=\"{}\" width=\"100\" height=\"100\" fill=\"{}\"/>\n",
                location.x * 100,
                location.y * 100,
                color
            ));

            svg.push_str(&format!(
                "\t<text x=\"{}\" y=\"{}\" fill=\"black\" font-size=\"50\">{}</text>\n",
                location.x * 100 + 10,
                location.y * 100 + 70,
                node.id
            ));
        }

        // draw lines for each netlist edge
        for edge in self.netlist.graph.edge_references() {
            let source_idx = edge.source();
            let target_idx = edge.target();

            let source = self.netlist.graph.node_weight(source_idx).unwrap();
            let target = self.netlist.graph.node_weight(target_idx).unwrap();

            let source_location = self.solution_map.get(source).unwrap();
            let target_location = self.solution_map.get(target).unwrap();

            svg.push_str(&format!(
                "\t<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" style=\"stroke:rgb(0,0,0);stroke-width:4\" />\n",
                source_location.x * 100 + 50,
                source_location.y * 100 + 50,
                target_location.x * 100 + 50,
                target_location.y * 100 + 50
            ));
        }

        svg.push_str("</svg>\n");

        svg
    }

    pub fn get_unplaced_nodes(&self) -> Vec<NetlistNode> {
        let mut unplaced_nodes: Vec<NetlistNode> = Vec::new();

        for node in self.netlist.graph.node_weights() {
            if !self.solution_map.contains_key(node) {
                unplaced_nodes.push(node.clone());
            }
        }

        unplaced_nodes
    }

    pub fn get_possible_sites(&self, macro_type: MacroType) -> Vec<FPGALayoutCoordinate> {
        let mut possible_sites = Vec::new();

        let mut placed_locations = FxHashSet::default();
        for location in self.solution_map.values() {
            placed_locations.insert(location.clone());
        }

        for x in 0..self.layout.width {
            for y in 0..self.layout.height {
                // check if the location is unplaced
                if placed_locations.contains(&FPGALayoutCoordinate::new(x, y)) {
                    continue;
                }

                let layout_type = self
                    .layout
                    .map
                    .get(&FPGALayoutCoordinate::new(x, y))
                    .unwrap();

                match layout_type {
                    FPGALayoutType::MacroType(layout_macro_type) => {
                        if layout_macro_type == &macro_type {
                            possible_sites.push(FPGALayoutCoordinate::new(x, y));
                        }
                    }
                    FPGALayoutType::EMPTY => {}
                }
            }
        }

        possible_sites
    }

    pub fn place_node(&mut self, node: NetlistNode, location: FPGALayoutCoordinate) {
        self.solution_map.insert(node, location);
    }

    pub fn valid(&self) -> bool {
        let netlist_nodes: Vec<NetlistNode> = self.netlist.all_nodes();
        let netlist_nodes_ids = netlist_nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<u32>>();

        // Check that all the nodes in the netlist are in the solution map
        for node in self.netlist.graph.node_weights() {
            if !self.solution_map.contains_key(&node) {
                return false;
            }
        }

        // Check that all the nodes in the solution map are in the netlist
        for node in self.solution_map.keys() {
            if !netlist_nodes_ids.contains(&node.id) {
                return false;
            }
        }

        // check that each location in the layout is only used once
        let mut used_locations = FxHashSet::default();
        for location in self.solution_map.values() {
            if used_locations.contains(location) {
                return false;
            }
            used_locations.insert(location.clone());
        }

        // check that each node in the netlist is only used once
        let mut used_nodes = FxHashSet::default();
        for node in self.solution_map.keys() {
            if used_nodes.contains(node) {
                return false;
            }
            used_nodes.insert(node.clone());
        }

        // check that nodes are placed on the correct type of macro
        for (node, location) in self.solution_map.iter() {
            let layout_type: FPGALayoutType = self.layout.get(location).unwrap();

            match layout_type {
                FPGALayoutType::MacroType(macro_type) => {
                    if node.macro_type != macro_type {
                        return false;
                    }
                }
                FPGALayoutType::EMPTY => return false,
            }
        }

        true
    }
}

pub fn gen_random_placement<'a>(
    layout: &'a FPGALayout,
    netlist: &'a NetlistGraph,
) -> PlacementSolution<'a> {
    let mut solution = PlacementSolution::new(layout, netlist);

    let mut rng = rand::thread_rng();

    let count_summary_layout = solution.layout.count_summary();
    let count_summary_netlist = solution.netlist.count_summary();

    for &macro_type in &[
        MacroType::CLB,
        MacroType::DSP,
        MacroType::BRAM,
        MacroType::IO,
    ] {
        assert!(
            count_summary_layout
                .get(&FPGALayoutType::MacroType(macro_type))
                .unwrap()
                >= count_summary_netlist.get(&macro_type).unwrap()
        );
    }

    for node in solution.netlist.all_nodes() {
        let possible_sites = solution.get_possible_sites(node.macro_type);
        let location: FPGALayoutCoordinate = possible_sites[rng.gen_range(0..possible_sites.len())];
        solution.place_node(node, location);
    }

    assert!(solution.valid());

    solution
}

pub fn gen_simple_placement<'a>(
    layout: &'a FPGALayout,
    netlist: &'a NetlistGraph,
) -> PlacementSolution<'a> {
    // itterate over the netlist nodes
    // placethem in the first spot in the layout in the top left corner
    let mut solution = PlacementSolution::new(layout, netlist);

    let count_summary_layout = solution.layout.count_summary();
    let count_summary_netlist = solution.netlist.count_summary();

    for &macro_type in &[
        MacroType::CLB,
        MacroType::DSP,
        MacroType::BRAM,
        MacroType::IO,
    ] {
        assert!(
            count_summary_layout
                .get(&FPGALayoutType::MacroType(macro_type))
                .unwrap()
                >= count_summary_netlist.get(&macro_type).unwrap()
        );
    }

    let nodes = solution.netlist.all_nodes();
    for node in nodes {
        let possible_sites = solution.get_possible_sites(node.macro_type);
        // get the site with the min manhattan distance to the origin
        let location = possible_sites
            .iter()
            .min_by(|a, b| {
                let a_distance = a.x + a.y;
                let b_distance = b.x + b.y;
                a_distance.cmp(&b_distance)
            })
            .unwrap();

        solution.place_node(node, location.clone());
    }

    assert!(solution.valid());

    solution
}

pub struct Renderer {
    pub svg_renders: Vec<String>,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            svg_renders: Vec::new(),
        }
    }

    pub fn add_frame(&mut self, svg: String) {
        self.svg_renders.push(svg);
    }

    pub fn render_to_video(&self, filename: &str, framerate: f64) {
        println!("Rendering to video");

        // tmpdir
        let dir = tempdir().unwrap();
        let frame_dir = dir.path().join("frames");
        std::fs::create_dir(&frame_dir).unwrap();

        let mut input_frames_svg_paths = Vec::new();

        for (frame_number, svg) in self.svg_renders.iter().enumerate() {
            // write the svg to a file
            let frame_fp = frame_dir.join(format!("frame_{}.svg", frame_number));
            input_frames_svg_paths.push(frame_fp.clone());
            std::fs::write(&frame_fp, svg).expect("Unable to write file");
        }

        let num_threads = 64;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        pool.install(|| {
            input_frames_svg_paths.par_iter().for_each(|svg_fp| {
                let png_fp = svg_fp.with_extension("png");
                println!("Converting {:?} to {:?} ... ", svg_fp, png_fp);
                let _output: std::process::Output = std::process::Command::new("magick")
                    .arg("convert")
                    .arg("-size")
                    .arg("800x800")
                    .arg(svg_fp)
                    .arg(png_fp)
                    .output()
                    .expect("failed to execute magick");
            });
        });

        // use ffmpeg to convert the frames to a video
        let mut ffmpeg_cmd = Command::new("ffmpeg");
        ffmpeg_cmd.arg("-y");
        ffmpeg_cmd.arg("-framerate");
        ffmpeg_cmd.arg(format!("{}", framerate));
        ffmpeg_cmd.arg("-i");
        ffmpeg_cmd.arg(frame_dir.join("frame_%d.png").to_str().unwrap());
        ffmpeg_cmd.arg("-c:v");
        ffmpeg_cmd.arg("libx264");
        ffmpeg_cmd.arg("-pix_fmt");
        ffmpeg_cmd.arg("yuv420p");
        ffmpeg_cmd.arg(format!("{}.mp4", filename));

        let child = ffmpeg_cmd.spawn().expect("failed to execute ffmpeg");
        child.wait_with_output().expect("failed to wait on ffmpeg");
    }
}

// fn placer_sa(
//     initial_solution: PalcementSolution,
//     n_steps: u32,
//     renderer: &mut Renderer,
//     render_frequency: u32,
// ) -> PalcementSolution {
//     let mut current_solution = initial_solution.clone();

//     renderer.add_frame(initial_solution.render_svg());

//     for i in 0..n_steps {
//         println!("Step: {}", i);

//         let mut new_solution = current_solution.clone();

//         // make a list to the actions to take
//         let actions: &[PlacementAction] = &[PlacementAction::MOVE, PlacementAction::SWAP];

//         // randomly select an action
//         let mut rng = rand::thread_rng();
//         let action: PlacementAction = actions[rng.gen_range(0..actions.len())];

//         println!("Action: {:?} ", action);
//         // take the action
//         new_solution.action(action);

//         let delta_cost = new_solution.cost_bb() - current_solution.cost_bb();

//         if delta_cost < 0.0 {
//             current_solution = new_solution;
//             println!("Delta Cost: {} (ACCEPT)", delta_cost);
//         } else {
//             println!("Delta Cost: {} (REJECT)", delta_cost);
//         }

//         println!("Current Cost: {:?}", current_solution.cost_bb());
//         println!();

//         if i % render_frequency == 0 {
//             renderer.add_frame(current_solution.render_svg());
//         }
//     }

//     current_solution
// }

pub fn placer_sa<'a>(
    initial_solution: PlacementSolution<'a>,
    n_steps: u32,
    renderer: &mut Renderer,
    render_frequency: u32,
    n_neighbors: usize, // number of neighbors to explore in parallel
) -> PlacementSolution<'a> {
    let mut current_solution = initial_solution.clone();
    renderer.add_frame(initial_solution.render_svg());

    for i in 0..n_steps {
        println!("Step: {}", i);

        let actions: &[PlacementAction] = &[PlacementAction::MOVE, PlacementAction::SWAP];

        // Generate n_neighbors neighboring solutions in parallel
        let new_solutions: Vec<_> = (0..n_neighbors)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();
                let mut new_solution = current_solution.clone();
                let action: PlacementAction = actions[rng.gen_range(0..actions.len())];
                new_solution.action(action);
                let new_cost = new_solution.cost_bb(); // Compute the cost first
                (new_solution, new_cost) // Then use both in the tuple
            })
            .collect();

        // Find the best new solution based on delta_cost
        if let Some((best_new_solution, best_new_cost)) =
            new_solutions.into_iter().min_by(|(_, cost1), (_, cost2)| {
                (cost1 - current_solution.cost_bb())
                    .partial_cmp(&(cost2 - current_solution.cost_bb()))
                    .unwrap()
            })
        {
            let delta_cost = best_new_cost - current_solution.cost_bb();
            if delta_cost < 0.0 {
                current_solution = best_new_solution;
                println!("Delta Cost: {} (ACCEPT)", delta_cost);
            } else {
                println!("Delta Cost: {} (REJECT)", delta_cost);
            }
        }

        println!("Current Cost: {:?}", current_solution.cost_bb());
        println!();

        if i % render_frequency == 0 {
            renderer.add_frame(current_solution.render_svg());
        }
    }

    current_solution
}

pub fn fast_sa_placer(
    initial_solution: PlacementSolution,
    n_steps: u32,
    n_neighbors: usize, // number of neighbors to explore at each step
    verbose: bool,
) -> PlacementSolution {
    let mut current_solution = initial_solution.clone();

    let mut rng = rand::thread_rng();
    let actions: &[PlacementAction] = &[
        PlacementAction::MOVE,
        PlacementAction::SWAP,
        PlacementAction::MOVE_DIRECTED,
    ];

    for _i in 0..n_steps {
        // randomly select an actions
        let actions = actions.choose_multiple(&mut rng, n_neighbors);

        // generate new solutions
        let new_solutions = actions.map(|action| {
            let mut new_solution = current_solution.clone();
            new_solution.action(*action);
            new_solution
        });
        let best_solution = new_solutions
            .min_by(|sol1, sol2| {
                (sol1.cost_bb() - current_solution.cost_bb())
                    .partial_cmp(&(sol2.cost_bb() - current_solution.cost_bb()))
                    .unwrap()
            })
            .unwrap();

        let best_delta = best_solution.cost_bb() - current_solution.cost_bb();
        let mut delta = 0.0;
        if best_delta < 0.0 {
            current_solution = best_solution;
            delta = best_delta;
        }

        // println!("Current Itteration: {:?}", _i);
        // println!("Delta Cost: {:?}", delta);
        // println!("Current Cost: {:?}", current_solution.cost_bb());
        if verbose {
            if _i % 10 == 0 {
                println!("Current Itteration: {:?}", _i);
                println!("Delta Cost: {:?}", delta);
                println!("Current Cost: {:?}", current_solution.cost_bb());
            }
        }
    }

    current_solution
}
