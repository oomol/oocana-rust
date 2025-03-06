use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct Slot {
    pub id: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Block {
    pub id: String,
    #[serde(default)]
    pub in_slots: Vec<Slot>,
    #[serde(default)]
    pub out_slots: Vec<Slot>,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
    pub pkg: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Edge {
    pub id: String,
    pub from_block: String,
    pub from_slot: String,
    pub to_block: String,
    pub to_slot: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GraphSerializable {
    pub id: String,
    pub blocks: Vec<Block>,
    pub edges: Vec<Edge>,
}

#[derive(Debug)]
pub struct Graph {
    pub id: String,
    pub blocks: HashMap<String, Block>,
    pub edges: HashMap<String, Edge>,
}

impl Graph {
    pub fn new(data: GraphSerializable) -> Self {
        let pipeline_graph = Graph {
            id: data.id,
            blocks: data
                .blocks
                .into_iter()
                .map(|b| (b.id.clone(), b))
                .collect::<HashMap<_, _>>(),
            edges: data
                .edges
                .into_iter()
                .map(|e| (e.id.clone(), e))
                .collect::<HashMap<_, _>>(),
        };

        pipeline_graph
    }

    pub fn get_block(&self, id: &str) -> Option<&Block> {
        self.blocks.get(id)
    }

    pub fn find_out_edges(&self, from_block_id: &str, from_slot_id: Option<&str>) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| {
                e.from_block == from_block_id
                    && match from_slot_id {
                        Some(slot_id) => e.from_slot == slot_id,
                        None => true,
                    }
            })
            .collect()
    }

    pub fn find_init_blocks(&self) -> Vec<&Block> {
        self.blocks
            .values()
            .filter(|b| b.in_slots.len() <= 0)
            .collect()
    }
}
