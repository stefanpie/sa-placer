use rustc_hash::FxHashMap;

use super::fpga_layout::MacroType;

// mod super::fpga_layout;

use rand::seq::SliceRandom;
use rand::Rng;
use rustworkx_core::generators::gnp_random_graph;
use rustworkx_core::petgraph;

#[derive(Clone, Hash, PartialEq, Eq, Debug, Copy)]
pub struct NetlistNode {
    pub id: u32,
    pub macro_type: MacroType,
}

#[derive(Debug, Clone)]

pub struct NetlistGraph {
    pub graph: petgraph::graph::DiGraph<NetlistNode, ()>,
}

impl NetlistGraph {
    // fn new() -> NetlistGraph {
    //     NetlistGraph {
    //         graph: petgraph::graph::DiGraph::new(),
    //     }
    // }

    pub fn all_nodes(&self) -> Vec<NetlistNode> {
        self.graph.node_weights().cloned().collect()
    }

    pub fn count_summary(&self) -> FxHashMap<MacroType, u32> {
        let mut count_summary = FxHashMap::default();

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

pub fn build_simple_netlist(n_nodes: u32, n_io: u32, n_bram: u32) -> NetlistGraph {
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

    // connect any dis nodes
    let unconnected_node_indices: Vec<_> = netlist
        .graph
        .node_indices()
        .filter(|node_idx| netlist.graph.neighbors(*node_idx).count() == 0)
        .collect();

    let connected_node_indices: Vec<_> = netlist
        .graph
        .node_indices()
        .filter(|node_idx| netlist.graph.neighbors(*node_idx).count() > 0)
        .collect();

    for unconnected_node_idx in unconnected_node_indices {
        let connected_node_idx = connected_node_indices
            .choose(&mut rng)
            .expect("No connected nodes found");

        netlist
            .graph
            .add_edge(*connected_node_idx, unconnected_node_idx, ());
    }

    netlist
}
