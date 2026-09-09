//! Dependency-first scheduling with frozen, declaration-ordered layers.

use std::collections::BTreeSet;

/// Return a schedule, or a closed path identifying an actual dependency cycle.
pub(crate) fn schedule(dependencies: &[BTreeSet<usize>]) -> Result<Vec<usize>, Vec<usize>> {
    let mut emitted = vec![false; dependencies.len()];
    let mut order = Vec::with_capacity(dependencies.len());
    while order.len() < dependencies.len() {
        let layer = dependencies
            .iter()
            .enumerate()
            .filter_map(|(node, edges)| {
                (!emitted[node] && edges.iter().all(|&dependency| emitted[dependency]))
                    .then_some(node)
            })
            .collect::<Vec<_>>();
        if layer.is_empty() {
            // Every remaining node has a remaining dependency. Following those
            // edges in a finite graph must revisit a node, identifying a cycle.
            let mut path = Vec::new();
            let mut node = emitted.iter().position(|done| !done).unwrap_or_else(|| {
                unreachable!("incomplete schedule has at least one unprocessed node")
            });
            loop {
                if let Some(start) = path.iter().position(|&previous| previous == node) {
                    let mut cycle = path[start..].to_vec();
                    cycle.push(node);
                    return Err(cycle);
                }
                path.push(node);
                node = *dependencies[node].iter().find(|&&next| !emitted[next])
                    .unwrap_or_else(|| unreachable!("empty ready layer means every remaining node has a remaining dependency"));
            }
        }
        for &node in &layer {
            emitted[node] = true;
        }
        order.extend(layer);
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezes_layers_and_preserves_source_order() {
        let dependencies = [BTreeSet::from([1]), BTreeSet::new(), BTreeSet::new()];
        assert_eq!(schedule(&dependencies), Ok(vec![1, 2, 0]));
    }

    #[test]
    fn reports_cycle_without_blocked_descendants() {
        let dependencies = [
            BTreeSet::from([1]),
            BTreeSet::from([2]),
            BTreeSet::from([1]),
        ];
        assert_eq!(schedule(&dependencies), Err(vec![1, 2, 1]));
    }

    #[test]
    fn exhaustively_checks_all_four_node_graphs() {
        for mask in 0_u32..(1 << 16) {
            let graph = (0..4)
                .map(|node| {
                    (0..4)
                        .filter(|&dependency| mask & (1 << (node * 4 + dependency)) != 0)
                        .collect::<BTreeSet<_>>()
                })
                .collect::<Vec<_>>();
            match schedule(&graph) {
                Ok(order) => {
                    assert_eq!(order.len(), 4);
                    for (position, &node) in order.iter().enumerate() {
                        assert!(
                            graph[node]
                                .iter()
                                .all(|dep| order[..position].contains(dep))
                        );
                    }
                }
                Err(cycle) => {
                    assert_eq!(cycle.first(), cycle.last());
                    assert!(
                        cycle
                            .windows(2)
                            .all(|edge| graph[edge[0]].contains(&edge[1]))
                    );
                }
            }
        }
    }
}
