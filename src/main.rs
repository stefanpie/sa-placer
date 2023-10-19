use std::collections::HashMap;
use std::collections::HashSet;

use std::path::Path;
use std::process::exit;
use std::process::Command;

use tempfile::tempdir;

use rustworkx_core::generators::gnp_random_graph;
use rustworkx_core::petgraph;
use rustworkx_core::petgraph::visit::EdgeRef;

use rand::seq::IteratorRandom;
use rand::seq::SliceRandom;
use rand::Rng;

use resvg::*;
use tiny_skia::Pixmap;
use tiny_skia::Transform;
use usvg::Options;
use usvg::Tree;
use usvg::TreeParsing;

use rayon::prelude::*;

#[derive(Clone, Hash, PartialEq, Eq, Debug, Copy)]
enum MacroType {
    CLB,
    DSP,
    BRAM,
    IO,
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
enum FPGALayoutType {
    MacroType(MacroType),
    EMPTY,
}

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
struct FPGALayoutCoordinate {
    x: u32,
    y: u32,
}

impl FPGALayoutCoordinate {
    fn new(x: u32, y: u32) -> FPGALayoutCoordinate {
        FPGALayoutCoordinate { x, y }
    }
}

#[derive(Debug, Clone)]
struct FPGALayout {
    map: HashMap<FPGALayoutCoordinate, FPGALayoutType>,
    width: u32,
    height: u32,
}

impl FPGALayout {
    fn new(width: u32, height: u32) -> FPGALayout {
        FPGALayout {
            map: HashMap::new(),
            width,
            height,
        }
    }

    fn config_corners(&mut self, layout_type: FPGALayoutType) {
        self.map
            .insert(FPGALayoutCoordinate::new(0, 0), layout_type.clone());
        self.map.insert(
            FPGALayoutCoordinate::new(0, self.height - 1),
            layout_type.clone(),
        );
        self.map.insert(
            FPGALayoutCoordinate::new(self.width - 1, 0),
            layout_type.clone(),
        );
        self.map.insert(
            FPGALayoutCoordinate::new(self.width - 1, self.height - 1),
            layout_type.clone(),
        );
    }

    fn config_border(&mut self, layout_type: FPGALayoutType) {
        for x in 0..self.width {
            self.map
                .insert(FPGALayoutCoordinate::new(x, 0), layout_type.clone());
            self.map.insert(
                FPGALayoutCoordinate::new(x, self.height - 1),
                layout_type.clone(),
            );
        }

        for y in 0..self.height {
            self.map
                .insert(FPGALayoutCoordinate::new(0, y), layout_type.clone());
            self.map.insert(
                FPGALayoutCoordinate::new(self.width - 1, y),
                layout_type.clone(),
            );
        }
    }

    fn config_repeat(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        step_x: u32,
        step_y: u32,
        layout_type: FPGALayoutType,
    ) {
        for x in (x..(x + width)).step_by(step_x as usize) {
            for y in (y..(y + height)).step_by(step_y as usize) {
                if x >= self.width || y >= self.height {
                    continue;
                }
                self.map
                    .insert(FPGALayoutCoordinate::new(x, y), layout_type.clone());
            }
        }
    }

    fn valid(&mut self) -> bool {
        // make sure all entries are inside the width and height
        for coord in self.map.keys() {
            if coord.x >= self.width || coord.y >= self.height {
                return false;
            }
        }
        true
    }

    fn get(&self, coordinate: &FPGALayoutCoordinate) -> Option<FPGALayoutType> {
        if coordinate.x >= self.width || coordinate.y >= self.height {
            return None;
        }
        if !self.map.contains_key(coordinate) {
            return Some(FPGALayoutType::EMPTY);
        }
        self.map.get(coordinate).cloned()
    }

    fn count_summary(&self) -> HashMap<FPGALayoutType, u32> {
        let mut count_summary: HashMap<FPGALayoutType, u32> = HashMap::new();

        let mut empty_count = 0;
        let mut clb_count = 0;
        let mut dsp_count = 0;
        let mut bram_count = 0;
        let mut io_count = 0;

        for x in 0..self.width {
            for y in 0..self.height {
                let layout_type = self.get(&FPGALayoutCoordinate::new(x, y)).unwrap();

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => clb_count += 1,
                    FPGALayoutType::MacroType(MacroType::DSP) => dsp_count += 1,
                    FPGALayoutType::MacroType(MacroType::BRAM) => bram_count += 1,
                    FPGALayoutType::MacroType(MacroType::IO) => io_count += 1,
                    FPGALayoutType::EMPTY => empty_count += 1,
                }
            }
        }

        count_summary.insert(FPGALayoutType::EMPTY, empty_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::CLB), clb_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::DSP), dsp_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::BRAM), bram_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::IO), io_count);

        count_summary
    }

    fn render_summary(&mut self) -> String {
        let mut output: String = String::new();

        // print the x, y size of the descive
        // print the number of each type of macro type

        output.push_str("FPGA Layout Summary\n");
        output.push_str(&format!("Width: {}\n", self.width));
        output.push_str(&format!("Height: {}\n", self.height));

        let mut clb_count = 0;
        let mut dsp_count = 0;
        let mut bram_count = 0;
        let mut io_count = 0;
        let mut empty_count = 0;

        for x in 0..self.width {
            for y in 0..self.height {
                let layout_type = self
                    .map
                    .get(&FPGALayoutCoordinate::new(x, y))
                    .unwrap_or(&FPGALayoutType::EMPTY);

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => clb_count += 1,
                    FPGALayoutType::MacroType(MacroType::DSP) => dsp_count += 1,
                    FPGALayoutType::MacroType(MacroType::BRAM) => bram_count += 1,
                    FPGALayoutType::MacroType(MacroType::IO) => io_count += 1,
                    FPGALayoutType::EMPTY => empty_count += 1,
                }
            }
        }

        output.push_str(&format!("CLB Count: {}\n", clb_count));
        output.push_str(&format!("DSP Count: {}\n", dsp_count));
        output.push_str(&format!("BRAM Count: {}\n", bram_count));
        output.push_str(&format!("IO Count: {}\n", io_count));
        output.push_str(&format!("Empty Count: {}\n", empty_count));

        output
    }

    fn render_ascii(&self) -> String {
        let mut output = String::new();

        // Draw the top line
        for x in 0..self.width {
            if x == 0 {
                output.push_str("┌───");
            } else {
                output.push_str("┬───");
            }
        }
        output.push_str("┐\n");

        for y in 0..self.height {
            // Draw the cells
            for x in 0..self.width {
                let layout_type = self
                    .map
                    .get(&FPGALayoutCoordinate::new(x, y))
                    .unwrap_or(&FPGALayoutType::EMPTY);

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => output.push_str("│ C "),
                    FPGALayoutType::MacroType(MacroType::DSP) => output.push_str("│ D "),
                    FPGALayoutType::MacroType(MacroType::BRAM) => output.push_str("│ B "),
                    FPGALayoutType::MacroType(MacroType::IO) => output.push_str("│ I "),
                    FPGALayoutType::EMPTY => output.push_str("│   "),
                }
            }
            output.push_str("│\n");

            // Draw the line below the cells if we are not at the last row
            if y < self.height - 1 {
                for x in 0..self.width {
                    if x == 0 {
                        output.push_str("├───");
                    } else {
                        output.push_str("┼───");
                    }
                }
                output.push_str("┤\n");
            }
        }

        // Draw the bottom line
        for x in 0..self.width {
            if x == 0 {
                output.push_str("└───");
            } else {
                output.push_str("┴───");
            }
        }
        output.push_str("┘\n");

        output
    }
}

fn build_simple_fpga_layout(width: u32, height: u32) -> FPGALayout {
    let mut layout = FPGALayout::new(width, height);

    layout.config_border(FPGALayoutType::MacroType(MacroType::IO));
    layout.config_corners(FPGALayoutType::EMPTY);
    layout.config_repeat(
        1,
        1,
        width - 2,
        height - 2,
        1,
        1,
        FPGALayoutType::MacroType(MacroType::CLB),
    );

    // evey 10 columns should be BRAM
    layout.config_repeat(
        10,
        1,
        width - 2,
        height - 2,
        10,
        1,
        FPGALayoutType::MacroType(MacroType::BRAM),
    );

    assert!(layout.valid());

    layout
}

#[derive(Clone, Hash, PartialEq, Eq, Debug, Copy)]
struct NetlistNode {
    id: u32,
    macro_type: MacroType,
}

#[derive(Debug, Clone)]

struct NetlistGraph {
    graph: petgraph::graph::DiGraph<NetlistNode, ()>,
}

impl NetlistGraph {
    fn new() -> NetlistGraph {
        NetlistGraph {
            graph: petgraph::graph::DiGraph::new(),
        }
    }

    fn all_nodes(&self) -> Vec<NetlistNode> {
        self.graph.node_weights().cloned().collect()
    }

    fn count_summary(&self) -> HashMap<MacroType, u32> {
        let mut count_summary: HashMap<MacroType, u32> = HashMap::new();

        let mut clb_count = 0;
        let mut dsp_count = 0;
        let mut bram_count = 0;
        let mut io_count = 0;

        for node in self.graph.node_weights() {
            match node.macro_type {
                MacroType::CLB => clb_count += 1,
                MacroType::DSP => dsp_count += 1,
                MacroType::BRAM => bram_count += 1,
                MacroType::IO => io_count += 1,
            }
        }

        count_summary.insert(MacroType::CLB, clb_count);
        count_summary.insert(MacroType::DSP, dsp_count);
        count_summary.insert(MacroType::BRAM, bram_count);
        count_summary.insert(MacroType::IO, io_count);

        count_summary
    }
}

fn build_simple_netlist(n_nodes: u32, n_io: u32, n_bram: u32) -> NetlistGraph {
    let mut netlist = NetlistGraph {
        graph: gnp_random_graph(
            n_nodes as usize,
            0.02,
            None,
            || NetlistNode {
                id: rand::thread_rng().gen(),
                macro_type: MacroType::CLB,
            },
            || (), // default_edge_weight
        )
        .unwrap(),
    };

    let mut rng: rand::rngs::ThreadRng = rand::thread_rng();

    fn get_clb_node_indices(netlist: &NetlistGraph) -> Vec<petgraph::graph::NodeIndex> {
        netlist
            .graph
            .node_indices()
            .filter(|node_idx| {
                netlist.graph.node_weight(*node_idx).unwrap().macro_type == MacroType::CLB
            })
            .collect()
    }

    // pick n_io random clbs and change their type to io
    // use choose_multiple to avoid duplicates
    let io_node_indices: Vec<_> = get_clb_node_indices(&netlist)
        .choose_multiple(&mut rng, n_io as usize)
        .cloned()
        .collect();

    for node_idx in io_node_indices {
        let node = netlist.graph.node_weight_mut(node_idx).unwrap();
        node.macro_type = MacroType::IO;
    }

    // pick n_bram random clbs and change their type to bram
    // use choose_multiple to avoid duplicates
    let bram_node_indices: Vec<_> = get_clb_node_indices(&netlist)
        .choose_multiple(&mut rng, n_bram as usize)
        .cloned()
        .collect();

    for node_idx in bram_node_indices {
        let node = netlist.graph.node_weight_mut(node_idx).unwrap();
        node.macro_type = MacroType::BRAM;
    }

    netlist
}

#[derive(Debug, Clone, Copy)]
enum PlacementAction {
    MOVE,
    SWAP,
}

#[derive(Debug, Clone)]
struct PlacementSolution {
    layout: FPGALayout,
    netlist: NetlistGraph,
    solution_map: HashMap<NetlistNode, FPGALayoutCoordinate>,
}

impl PlacementSolution {
    fn new(layout: FPGALayout, netlist: NetlistGraph) -> PlacementSolution {
        PlacementSolution {
            layout,
            netlist,
            solution_map: HashMap::new(),
        }
    }

    fn action_move(&mut self) {
        let mut rng = rand::thread_rng();

        let node: NetlistNode =
            self.netlist.all_nodes()[rng.gen_range(0..self.netlist.graph.node_count())];

        let possible_sites = self.get_possible_sites(node.macro_type);
        if possible_sites.len() == 0 {
            return;
        }

        let location: FPGALayoutCoordinate = possible_sites[rng.gen_range(0..possible_sites.len())];

        self.solution_map.insert(node, location);
    }

    fn action_swap(&mut self) {
        let mut rng: rand::rngs::ThreadRng = rand::thread_rng();

        let node_a: NetlistNode =
            self.netlist.all_nodes()[rng.gen_range(0..self.netlist.graph.node_count())];

        // node b should be same type as node a
        let nodes_same_type: Vec<NetlistNode> = self
            .netlist
            .all_nodes()
            .iter()
            .filter(|node| node.macro_type == node_a.macro_type)
            .cloned()
            .collect();

        if nodes_same_type.len() == 0 {
            return;
        }

        let node_b: NetlistNode = nodes_same_type[rng.gen_range(0..nodes_same_type.len())];

        let loc_a = self.solution_map.get(&node_a).unwrap().clone();
        let loc_b = self.solution_map.get(&node_b).unwrap().clone();

        self.solution_map.insert(node_a, loc_b);
        self.solution_map.insert(node_b, loc_a);
    }

    fn action(&mut self, action: PlacementAction) {
        match action {
            PlacementAction::MOVE => self.action_move(),
            PlacementAction::SWAP => self.action_swap(),
        }
    }

    fn cost_bb(&self) -> f32 {
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

    fn render_svg(&self) -> String {
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

    fn get_unplaced_nodes(&self) -> Vec<NetlistNode> {
        let mut unplaced_nodes: Vec<NetlistNode> = Vec::new();

        for node in self.netlist.graph.node_weights() {
            if !self.solution_map.contains_key(node) {
                unplaced_nodes.push(node.clone());
            }
        }

        unplaced_nodes
    }

    fn get_possible_sites(&self, macro_type: MacroType) -> Vec<FPGALayoutCoordinate> {
        let mut possible_sites: Vec<FPGALayoutCoordinate> = Vec::new();

        let mut placed_locations: HashSet<FPGALayoutCoordinate> = HashSet::new();
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

    fn place_node(&mut self, node: NetlistNode, location: FPGALayoutCoordinate) {
        self.solution_map.insert(node, location);
    }

    fn valid(&self) -> bool {
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
        let mut used_locations: HashSet<FPGALayoutCoordinate> = HashSet::new();
        for location in self.solution_map.values() {
            if used_locations.contains(location) {
                return false;
            }
            used_locations.insert(location.clone());
        }

        // check that each node in the netlist is only used once
        let mut used_nodes: HashSet<NetlistNode> = HashSet::new();
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

fn gen_random_placement(layout: FPGALayout, netlist: NetlistGraph) -> PlacementSolution {
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

struct Renderer {
    svg_renders: Vec<String>,
}

impl Renderer {
    fn new() -> Renderer {
        Renderer {
            svg_renders: Vec::new(),
        }
    }

    fn add_frame(&mut self, svg: String) {
        self.svg_renders.push(svg);
    }

    fn render_to_video(&self, filename: &str, framerate: f64) {
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

fn placer_sa(
    initial_solution: PlacementSolution,
    n_steps: u32,
    renderer: &mut Renderer,
    render_frequency: u32,
    n_neighbors: usize, // number of neighbors to explore in parallel
) -> PlacementSolution {
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

fn main() {
    let mut layout = build_simple_fpga_layout(64, 64);

    let vis = layout.render_ascii();
    std::fs::write("fpga_layout_vis.txt", vis).expect("Unable to write file");

    let summary = layout.render_summary();
    std::fs::write("fpga_layout_summary.txt", summary).expect("Unable to write file");

    let netlist = build_simple_netlist(300, 30, 100);
    println!("Netlist Summary: {:?}", netlist.count_summary());

    let initial_solution = gen_random_placement(layout, netlist);
    // std::fs::write(
    //     "fpga_layout_solution_initial.svg",
    //     initial_solution.render_svg(),
    // )
    // .expect("Unable to write file");

    let mut renderer = Renderer::new();

    let _final_solution: PlacementSolution =
        placer_sa(initial_solution, 2000, &mut renderer, 10, 16);

    renderer.render_to_video("fpga_layout_solution", 30.0);
}
