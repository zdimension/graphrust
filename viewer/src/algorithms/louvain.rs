use crate::log;
use ahash::AHashMap;
use itertools::Itertools;
use rand::seq::SliceRandom;
/// Louvain algorithm
/// Ported from https://github.com/ledyba/cpp-louvain-fast
/// Licensed under the AGPLv3 license, see https://github.com/ledyba/cpp-louvain-fast/blob/master/LICENSE
use rand::thread_rng;
use crate::app::Person;

pub struct Graph {
    pub nodes: Vec<Community>,
    pub total_links: usize,
}

#[derive(Copy, Clone, Default)]
pub struct PersonId(pub usize);
#[derive(Copy, Clone)]
pub struct CommunityId(pub usize);

const PRECISION: f32 = 0.0;
const RESOLUTION: f32 = 1.0; // the lower the smaller the communities
const ITERATIONS: usize = 100; // iterations before giving up

fn merge(nodes: &Vec<Community>, idxs: &Vec<CommunityId>) -> Vec<PersonId> {
    idxs.iter()
        .flat_map(|i| nodes[i.0].payload.as_ref().unwrap())
        .copied()
        .collect()
}

trait GraphNode {
    fn neighbors(&self) -> &Vec<usize>;
}

impl GraphNode for Person {
    fn neighbors(&self) -> &Vec<usize> {
        &self.neighbors
    }
}

impl Graph {
    pub fn new(persons: &Vec<impl GraphNode>) -> Self {
        let mut nodes = Vec::with_capacity(persons.len());
        let mut total_links = 0;
        for (i, pers) in persons.iter().enumerate() {
            let mut comm = Community::new(Some(vec![PersonId(i)]));
            comm.neighbors = pers.neighbors().iter().map(|&x| Edge { other: CommunityId(x), weight: 1 }).collect();
            nodes.push(comm);
            total_links += pers.neighbors().len();
        }
        Self { nodes, total_links }
    }

    fn next(mut self) -> Self {
        const MAX: usize = 50;

        let n_nodes = self.nodes.len();
        let mut tmp_comm = vec![0; n_nodes];
        {
            let g_total = self.total_links;
            let mut comm_total = vec![0; n_nodes];
            let mut order = vec![0; n_nodes];
            for i in 0..n_nodes {
                tmp_comm[i] = i;
                order[i] = i;
                comm_total[i] = self.nodes[i].degree;
            }
            let mut neigh_links = vec![0; n_nodes];
            let mut neigh_comm = Vec::with_capacity(n_nodes);

            let mut changed = n_nodes;
            let mut cnt = 0;
            let change_limit = n_nodes / 100;
            order.shuffle(&mut thread_rng());
            while changed > change_limit {
                if MAX > 0 && cnt >= MAX {
                    println!("Exceed limit pass");
                    break;
                }
                cnt += 1;
                changed = 0;
                for &pos in &order {
                    let node = &self.nodes[pos];
                    let node_tmp_comm = tmp_comm[pos];
                    let node_degree = node.degree;
                    for &comm in &neigh_comm {
                        neigh_links[comm] = 0;
                    }
                    neigh_comm.clear();
                    for link in &node.neighbors {
                        let to = tmp_comm[link.other.0];
                        let weight = link.weight;
                        if neigh_links[to] <= 0 {
                            neigh_comm.push(to);
                            neigh_links[to] = weight;
                        } else {
                            neigh_links[to] += weight;
                        }
                    }
                    let mut best_comm = node_tmp_comm;
                    let mut best_gain = PRECISION;
                    for &comm in &neigh_comm {
                        let gain = if comm == node_tmp_comm {
                            neigh_links[comm] as f32
                                - (comm_total[comm] - node_degree) as f32 * node_degree as f32
                                / g_total as f32
                        } else {
                            neigh_links[comm] as f32
                                - comm_total[comm] as f32 * node_degree as f32 / g_total as f32
                        };
                        if gain > best_gain {
                            best_gain = gain;
                            best_comm = comm;
                        }
                    }
                    if node_tmp_comm != best_comm {
                        changed += 1;
                        tmp_comm[pos] = best_comm;
                        comm_total[node_tmp_comm] -= node_degree;
                        comm_total[best_comm] += node_degree;
                    }
                }
            }
        }
        let mut old_comm_idx = Vec::with_capacity(self.nodes.len() / 10);
        let mut c2i = vec![0; n_nodes];
        let mut communities = Vec::with_capacity(self.nodes.len() / 10);
        for i in 0..n_nodes {
            let node_tmp_comm = tmp_comm[i];
            let c = c2i[node_tmp_comm];
            if c <= 0 {
                c2i[node_tmp_comm] = communities.len() + 1;
                old_comm_idx.push(node_tmp_comm);
                communities.push(Community {
                    children: vec![CommunityId(i)],
                    ..Community::new(None)
                });
            } else {
                communities[c - 1].children.push(CommunityId(i));
            }
        }
        for i in 0..communities.len() {
            let comm = &mut communities[i];
            let old_comm = old_comm_idx[i];
            let mut links = AHashMap::new();
            for cidx in &comm.children {
                let child = &mut self.nodes[cidx.0];
                child.parent = Some(i);
                comm.self_loops += child.self_loops;
                comm.degree += child.self_loops;
                for link in &child.neighbors {
                    let link_to_idx = link.other.0;
                    let weight = link.weight;
                    let c_link_to_comm_now = tmp_comm[link_to_idx];
                    comm.degree += weight;
                    if c_link_to_comm_now == old_comm {
                        comm.self_loops += weight;
                    } else {
                        *links.entry(c2i[c_link_to_comm_now] - 1).or_default() += weight;
                    }
                }
            }

            comm.neighbors = comm
                .neighbors
                .splice(
                    0..0,
                    links
                        .iter()
                        .map(|(&comm, &weight)| Edge { other: CommunityId(comm), weight }),
                )
                .collect();
            comm.payload = Some(merge(&self.nodes, &comm.children));
        }
        Self {
            nodes: communities,
            total_links: self.total_links,
        }
    }

    fn stats(&self) -> (usize, usize) {
        (self.nodes.len(), self.total_links)
    }

    /*fn modularity(&self) -> f32 {
        self.nodes
            .iter()
            .map(|c| {
                let x = 0;
                todo!();
                0.0
            })
            .sum()
    }*/

    pub fn louvain(mut self) -> Self {
        for i in 0..ITERATIONS {
            let old_stats = self.stats();
            self = self.next();
            /*log!(
                "Louvain iteration {} done : {:?} → {:?}",
                i,
                old_stats,
                self.stats()
            );*/
            if old_stats == self.stats() {
                return self;
            }
        }
        panic!("Graph did not converge after {} iterations", ITERATIONS);
    }
}

pub struct Edge {
    other: CommunityId,
    weight: usize, // TODO: always 1?
}

pub struct Community {
    pub payload: Option<Vec<PersonId>>,
    pub children: Vec<CommunityId>,
    neighbors: Vec<Edge>,
    degree: usize,
    parent: Option<usize>,
    self_loops: usize,
}

impl Community {
    fn new(payload: Option<Vec<PersonId>>) -> Self {
        Self {
            payload,
            children: Vec::new(),
            neighbors: Vec::new(),
            degree: 0,
            parent: None,
            self_loops: 0,
        }
    }
}