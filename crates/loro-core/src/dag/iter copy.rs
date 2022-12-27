use super::DagUtils;
use std::ops::Range;

use crate::version::IdSpanVector;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdHeapItem {
    id: ID,
    lamport: Lamport,
}

impl PartialOrd for IdHeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.lamport.cmp(&other.lamport).reverse())
    }
}

impl Ord for IdHeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.lamport.cmp(&other.lamport).reverse()
    }
}

pub(crate) fn iter_dag_with_vv<T, D: Dag<Node = T>>(dag: &D) -> DagIteratorVV<'_, T> {
    DagIteratorVV {
        dag,
        vv_map: Default::default(),
        heap: BinaryHeap::new(),
    }
}

pub(crate) fn iter_dag<T>(dag: &dyn Dag<Node = T>) -> DagIterator<'_, T> {
    DagIterator {
        dag,
        visited: VersionVector::new(),
        heap: BinaryHeap::new(),
    }
}

pub struct DagIterator<'a, T> {
    dag: &'a dyn Dag<Node = T>,
    /// Because all deps' lamports are smaller than current node's lamport.
    /// We can use the lamport to sort the nodes so that each node's deps are processed before itself.
    ///
    /// The ids in this heap are start ids of nodes. It won't be a id pointing to the middle of a node.
    heap: BinaryHeap<IdHeapItem>,
    visited: VersionVector,
}

/// Should only use it on debug, because it's slow and likely to use lots of mem
impl<'a, T: DagNode> Iterator for DagIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.visited.is_empty() {
            if self.dag.vv().len() == 0 {
                return None;
            }

            for (&client_id, _) in self.dag.vv().iter() {
                if let Some(node) = self.dag.get(ID::new(client_id, 0)) {
                    self.heap.push(IdHeapItem {
                        id: ID::new(client_id, 0),
                        lamport: node.lamport(),
                    });
                }

                self.visited.insert(client_id, 0);
            }
        }

        if !self.heap.is_empty() {
            let item = self.heap.pop().unwrap();
            let id = item.id;
            let node = self.dag.get(id).unwrap();
            // push next node from the same client to the heap
            let next_id = id.inc(node.content_len() as i32);
            if self.dag.contains(next_id) {
                let next_node = self.dag.get(next_id).unwrap();
                self.heap.push(IdHeapItem {
                    id: next_id,
                    lamport: next_node.lamport(),
                });
            }

            return Some(node);
        }

        None
    }
}

pub(crate) struct DagIteratorVV<'a, T> {
    dag: &'a dyn Dag<Node = T>,
    /// we should keep every nodes starting id inside this map
    vv_map: FxHashMap<ID, VersionVector>,
    /// Because all deps' lamports are smaller than current node's lamport.
    /// We can use the lamport to sort the nodes so that each node's deps are processed before itself.
    ///
    /// The ids in this heap are start ids of nodes. It won't be a id pointing to the middle of a node.
    heap: BinaryHeap<IdHeapItem>,
}

/// Should only use it on debug, because it's slow and likely to use lots of mem
impl<'a, T: DagNode> Iterator for DagIteratorVV<'a, T> {
    type Item = (&'a T, VersionVector);

    fn next(&mut self) -> Option<Self::Item> {
        if self.vv_map.is_empty() {
            if self.dag.vv().len() == 0 {
                return None;
            }

            for (&client_id, _) in self.dag.vv().iter() {
                let vv = VersionVector::new();
                if let Some(node) = self.dag.get(ID::new(client_id, 0)) {
                    if node.lamport() == 0 {
                        self.vv_map.insert(ID::new(client_id, 0), vv.clone());
                    }

                    self.heap.push(IdHeapItem {
                        id: ID::new(client_id, 0),
                        lamport: node.lamport(),
                    });
                }
            }
        }

        if !self.heap.is_empty() {
            let item = self.heap.pop().unwrap();
            let id = item.id;
            let node = self.dag.get(id).unwrap();
            debug_assert_eq!(id, node.id_start());
            let mut vv = {
                // calculate vv
                let mut vv = None;
                for &dep_id in node.deps() {
                    let dep = self.dag.get(dep_id).unwrap();
                    let dep_vv = self.vv_map.get(&dep.id_start()).unwrap();
                    if vv.is_none() {
                        vv = Some(dep_vv.clone());
                    } else {
                        vv.as_mut().unwrap().merge(dep_vv);
                    }

                    if dep.id_start() != dep_id {
                        vv.as_mut().unwrap().set_last(dep_id);
                    }
                }

                vv.unwrap_or_else(VersionVector::new)
            };

            vv.try_update_last(id);
            self.vv_map.insert(id, vv.clone());

            // push next node from the same client to the heap
            let next_id = id.inc(node.content_len() as i32);
            if self.dag.contains(next_id) {
                let next_node = self.dag.get(next_id).unwrap();
                self.heap.push(IdHeapItem {
                    id: next_id,
                    lamport: next_node.lamport(),
                });
            }

            return Some((node, vv));
        }

        None
    }
}

/// Visit every span in the target IdSpanVector.
/// It's guaranteed that the spans are visited in causal order, and each span is visited only once.
/// When visiting a span, we will checkout to the version where the span was created
pub(crate) struct DagCausalIter<'a, Dag> {
    dag: &'a Dag,
    frontier: SmallVec<[ID; 2]>,
    target: IdSpanVector,
    heap: BinaryHeap<IdHeapItem>,
}

#[derive(Debug)]
pub(crate) struct IterReturn<'a, T> {
    pub retreat: IdSpanVector,
    pub forward: IdSpanVector,
    /// data is a reference, it need to be sliced by the counter_range to get the underlying data
    pub data: &'a T,
    /// data[slice] is the data we want to return
    pub slice: Range<i32>,
}

impl<'a, T: DagNode, D: Dag<Node = T>> DagCausalIter<'a, D> {
    pub fn new(dag: &'a D, from: SmallVec<[ID; 2]>, target: IdSpanVector) -> Self {
        let mut heap = BinaryHeap::new();
        for id in target.iter() {
            if id.1.content_len() > 0 {
                let id = id.id_start();
                let node = dag.get(id).unwrap();
                let diff = id.counter - node.id_start().counter;
                heap.push(IdHeapItem {
                    id,
                    lamport: node.lamport() + diff as Lamport,
                });
            }
        }

        Self {
            dag,
            frontier: from,
            target,
            heap,
        }
    }
}

impl<'a, T: DagNode + 'a, D: Dag<Node = T>> Iterator for DagCausalIter<'a, D> {
    type Item = IterReturn<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.heap.is_empty() {
            debug_assert_eq!(
                0,
                self.target
                    .iter()
                    .map(|x| x.1.content_len() as i32)
                    .sum::<i32>()
            );
            return None;
        }

        let node_id = self.heap.pop().unwrap().id;
        let target_span = self.target.get_mut(&node_id.client_id).unwrap();
        debug_assert_eq!(
            node_id.counter,
            target_span.min(),
            "{} {:?}",
            node_id,
            target_span
        );

        // node_id may points into the middle of the node, we need to slice
        let node = self.dag.get(node_id).unwrap();
        // node start_id may be smaller than node_id
        let counter = node.id_span().counter;
        let slice_from = if counter.start < target_span.start {
            target_span.start - counter.start
        } else {
            0
        };
        let slice_end = if counter.end < target_span.end {
            counter.end - counter.start
        } else {
            target_span.end - counter.start
        };
        assert!(slice_end > slice_from);
        let last_counter = node.id_last().counter;
        target_span.set_start(last_counter + 1);
        if target_span.content_len() > 0 {
            let next_id = ID::new(node_id.client_id, last_counter + 1);
            let next_node = self.dag.get(next_id).unwrap();
            self.heap.push(IdHeapItem {
                id: next_id,
                lamport: next_node.lamport()
                    + (next_id.counter - next_node.id_start().counter) as Lamport,
            });
        }

        let deps: SmallVec<[_; 2]> = if slice_from == 0 {
            node.deps().iter().copied().collect()
        } else {
            smallvec::smallvec![node.id_start().inc(slice_from - 1)]
        };

        let path = self.dag.find_path(&self.frontier, &deps);
        println!(
            "########\nfrontier: {:?} deps: {:?} path: {:?}\n#####",
            &self.frontier, &deps, &path
        );
        debug_log::group!("Dag Causal");
        debug_log::debug_dbg!(&deps);
        debug_log::debug_dbg!(&path);
        debug_log::group_end!();
        // NOTE: we expect user to update the tracker, to apply node, after visiting the node
        self.frontier = smallvec::smallvec![node.id_start().inc(slice_end - 1)];
        Some(IterReturn {
            retreat: path.left,
            forward: path.right,
            data: node,
            slice: slice_from..slice_end,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::{
        dag::DagUtils,
        id::{Counter, ID},
        log_store::{EncodeConfig, EncodeMode},
        LoroCore,
    };

    #[test]
    fn my_case() {
        let mut loro_a = LoroCore::new(Default::default(), Some(1));
        let mut loro_b = LoroCore::new(Default::default(), Some(2));
        let mut loro_c = LoroCore::new(Default::default(), Some(3));

        let mut text_a = loro_a.get_text("text");
        let mut text_b = loro_b.get_text("text");
        let mut text_c = loro_c.get_text("text");

        text_a.insert(&loro_a, 0, "a1").unwrap();
        text_a.insert(&loro_a, 0, "a2").unwrap();
        text_a.insert(&loro_a, 0, "a3").unwrap();

        text_b.insert(&loro_b, 0, "b1").unwrap();

        loro_c
            .decode(
                &loro_b
                    .encode(EncodeConfig::new(EncodeMode::Snapshot, None))
                    .unwrap(),
            )
            .unwrap();

        text_c.insert(&loro_c, 0, "c1").unwrap();
        let from: Vec<ID> = {
            let m = loro_c.log_store.try_read().unwrap();
            let f = m.frontiers();
            f.to_vec()
        };
        let from_vv = loro_c.vv();
        println!("c start vv: {:?}", loro_c.vv());

        text_b.insert(&loro_b, 0, "b2").unwrap();
        text_b.insert(&loro_b, 0, "b3").unwrap();

        loro_b
            .decode(
                &loro_a
                    .encode(EncodeConfig::new(EncodeMode::Updates(loro_b.vv()), None))
                    .unwrap(),
            )
            .unwrap();

        text_b.insert(&loro_b, 0, "b4").unwrap();

        text_c.insert(&loro_c, 0, "c2").unwrap();

        loro_c
            .decode(
                &loro_b
                    .encode(EncodeConfig::new(EncodeMode::Updates(loro_c.vv()), None))
                    .unwrap(),
            )
            .unwrap();

        text_c.insert(&loro_c, 0, "c3").unwrap();
        let mut vv = from_vv.clone();

        println!(
            "from {:?}, diff {:?}",
            &from,
            loro_c.vv().diff(&from_vv).left
        );

        let store_c = loro_c.log_store.try_read().unwrap();

        for n in store_c.iter_causal(&from, loro_c.vv().diff(&from_vv).left) {
            println!("{:?}", &n);
            println!("data: {:?}", store_c.change_to_export_format(n.data));
            vv.retreat(&n.retreat);
            vv.forward(&n.forward);

            let start = n.slice.start;
            let end = n.slice.end;
            let change = n.data;

            vv.set_end(ID::new(
                change.id.client_id,
                end as Counter + change.id.counter,
            ));
            println!("{:?}\n", vv);
        }
    }
}