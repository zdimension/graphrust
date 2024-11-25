use crate::algorithms::AbstractNode;
use ahash::AHashSet;
use bit_set::BitSet;
use derivative::*;
use std::collections::VecDeque;

pub fn do_pathfinding(
    settings: PathSectionSettings,
    data: &[impl AbstractNode],
) -> Option<PathSectionResults> {
    let src_id = settings.path_src.unwrap();
    let dest_id = settings.path_dest.unwrap();
    let src = &data[src_id];
    let dest = &data[dest_id];

    let mutual: AHashSet<usize> = if settings.path_no_mutual {
        AHashSet::<_>::from_iter(src.neighbors().iter().copied())
            .intersection(&AHashSet::<_>::from_iter(dest.neighbors().iter().copied()))
            .copied()
            .collect()
    } else {
        AHashSet::new()
    };

    let exclude_set: AHashSet<usize> = AHashSet::from_iter(settings.exclude_ids.iter().cloned());

    let mut queue_f = VecDeque::new();
    let mut queue_b = VecDeque::new();
    let mut visited_f = BitSet::with_capacity(data.len());
    let mut visited_b = BitSet::with_capacity(data.len());
    let mut pred_f = vec![None; data.len()];
    let mut pred_b = vec![None; data.len()];

    visited_f.insert(src_id);
    visited_b.insert(dest_id);
    queue_f.push_back(src_id);
    queue_b.push_back(dest_id);

    let bfs = |current: usize,
               queue: &mut VecDeque<usize>,
               visited: &mut BitSet,
               pred: &mut Vec<Option<usize>>,
               visited_other: &BitSet| {
        let person = &data[current];
        for &nb_id in person.neighbors().iter() {
            if settings.path_no_direct
                && ((current, nb_id) == (src_id, dest_id) || (current, nb_id) == (dest_id, src_id))
            {
                continue;
            }

            if settings.path_no_mutual && mutual.contains(&nb_id) {
                continue;
            }

            if exclude_set.contains(&nb_id) {
                continue;
            }

            if !visited.contains(nb_id) {
                pred[nb_id] = Some(current);
                if visited_other.contains(nb_id) {
                    return Some(nb_id);
                }
                visited.insert(nb_id);
                queue.push_back(nb_id);
            }
        }
        None
    };

    let intersect = 'main: loop {
        // Balancing the bidirectional BFS (instead of visiting each k-neighborhood alternatively)
        // shortens the usual runtime on my machine for long paths (>11) from 500ms to 10ms.
        // Thanks to https://arxiv.org/pdf/2410.22186
        if queue_b.is_empty() || queue_f.is_empty() {
            return None;
        }
        if visited_b.len() < visited_f.len() {
            let mut queue_new_b = VecDeque::new();
            while let Some(id_b) = queue_b.pop_front() {
                if let Some(inter) = bfs(
                    id_b,
                    &mut queue_new_b,
                    &mut visited_b,
                    &mut pred_b,
                    &visited_f,
                ) {
                    break 'main inter;
                }
            }
            queue_b = queue_new_b;
        } else {
            let mut queue_new_f = VecDeque::new();
            while let Some(id_f) = queue_f.pop_front() {
                if let Some(inter) = bfs(
                    id_f,
                    &mut queue_new_f,
                    &mut visited_f,
                    &mut pred_f,
                    &visited_b,
                ) {
                    break 'main inter;
                }
            }
            queue_f = queue_new_f;
        }
    };

    let mut path = vec![intersect];
    let mut cur = intersect;
    while let Some(pred) = pred_f[cur] {
        path.push(pred);
        cur = pred;
    }
    path.reverse();
    cur = intersect;
    while let Some(pred) = pred_b[cur] {
        path.push(pred);
        cur = pred;
    }
    Some(PathSectionResults { path })
}

#[derive(Derivative)]
#[derivative(Default, Clone)]
pub struct PathSectionSettings {
    pub path_src: Option<usize>,
    pub path_dest: Option<usize>,
    pub exclude_ids: Vec<usize>,
    pub path_no_direct: bool,
    pub path_no_mutual: bool,
}

#[derive(Clone, Debug)]
pub struct PathSectionResults {
    pub path: Vec<usize>,
}
