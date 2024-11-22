use crate::algorithms::AbstractNode;
use ahash::AHashSet;
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
    let mut visited_f = vec![false; data.len()];
    let mut visited_b = vec![false; data.len()];
    let mut pred_f = vec![None; data.len()];
    let mut pred_b = vec![None; data.len()];
    let mut dist_f = vec![i32::MAX; data.len()];
    let mut dist_b = vec![i32::MAX; data.len()];

    visited_f[src_id] = true;
    visited_b[dest_id] = true;
    queue_f.push_back(src_id);
    queue_b.push_back(dest_id);
    dist_f[src_id] = 0;
    dist_b[dest_id] = 0;

    let bfs = |current: usize,
               queue: &mut VecDeque<usize>,
               visited: &mut Vec<bool>,
               pred: &mut Vec<Option<usize>>,
               visited_other: &Vec<bool>| {
        let person = &data[current];
        for &nb_id in person.neighbors().iter() {
            if settings.path_no_direct && current == src_id && nb_id == dest_id {
                continue;
            }

            if settings.path_no_mutual && mutual.contains(&nb_id) {
                continue;
            }

            if exclude_set.contains(&nb_id) {
                continue;
            }

            if !visited[nb_id] {
                visited[nb_id] = true;
                pred[nb_id] = Some(current);
                if visited_other[nb_id] {
                    return Some(nb_id);
                }
                queue.push_back(nb_id);
            }
        }
        None
    };

    let intersect = 'main: loop {
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

        if queue_new_f.is_empty() && queue_new_b.is_empty() {
            return None;
        }
        queue_f = queue_new_f;
        queue_b = queue_new_b;
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
