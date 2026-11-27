use std::collections::BTreeMap;

use pg_rules::trace::TraceHandle;
use serde_json::{json, Value};

use super::evidence::Group;

#[derive(Clone, Copy)]
pub(super) enum Parent {
    Root,
    Unavailable,
    Path(TraceHandle),
}

#[derive(Clone, Copy)]
struct Node {
    parent: Parent,
    depth: usize,
}

pub(super) struct Lineage {
    nodes: Vec<Node>,
    retained: BTreeMap<usize, usize>,
    max_nodes: usize,
    max_depth: usize,
}

pub(super) struct Plan {
    pub(super) parent: Value,
    pub(super) status: &'static str,
    pub(super) depth: Option<usize>,
    pub(super) depth_cut: bool,
    pub(super) join: Option<usize>,
    nodes: Vec<(usize, Node, Option<usize>)>,
}

impl Lineage {
    pub(super) fn new(max_depth: usize, bytes: usize) -> Self {
        Self {
            nodes: Vec::new(),
            retained: BTreeMap::new(),
            // Each stored node needs a path record of at least this size to be usable.
            max_nodes: bytes / 64,
            max_depth,
        }
    }

    pub(super) fn advance(&mut self, parent: Parent) -> Option<TraceHandle> {
        if self.nodes.len() >= self.max_nodes {
            return Some(TraceHandle::OMITTED_OBSERVATION);
        }
        let handle = TraceHandle::observation(u32::try_from(self.nodes.len()).ok()?)?;
        let depth = self
            .node(parent)
            .map_or(0, |node| node.depth.saturating_add(1));
        self.nodes.push(Node { parent, depth });
        Some(handle)
    }

    fn index(parent: Parent) -> Option<usize> {
        match parent {
            Parent::Path(id) if id != TraceHandle::OMITTED_OBSERVATION => Some(id.index() as usize),
            _ => None,
        }
    }

    fn node(&self, parent: Parent) -> Option<Node> {
        Self::index(parent).and_then(|index| self.nodes.get(index).copied())
    }

    fn wire_id(index: usize) -> usize {
        index * 2
    }

    fn parent_wire(parent: Parent, retained: bool) -> (Value, &'static str) {
        match parent {
            Parent::Root => (Value::Null, "root"),
            Parent::Unavailable => (Value::Null, "unavailable"),
            Parent::Path(_) if !retained => (Value::Null, "omitted"),
            Parent::Path(id) => (json!(Self::wire_id(id.index() as usize)), "retained"),
        }
    }

    fn plan_chain(
        &self,
        parent: Parent,
        max_links: usize,
        shallow: bool,
        nodes: &mut Vec<(usize, Node, Option<usize>)>,
        depth_cut: &mut bool,
    ) -> Option<usize> {
        let mut chain = Vec::new();
        let mut next = parent;
        let mut links = None;
        while let Some(index) = Self::index(next) {
            let saved_links = self.retained.get(&index).copied().or_else(|| {
                nodes.iter().find_map(|(id, _, parent_links)| {
                    (*id == index).then(|| parent_links.map_or(0, |count| count + 1))
                })
            });
            if let Some(saved_links) = saved_links {
                links = (chain.len() + 1 + saved_links <= max_links).then_some(saved_links);
                *depth_cut |= links.is_none();
                break;
            }
            if chain.len() >= max_links {
                *depth_cut = true;
                break;
            }
            if shallow {
                break;
            }
            let node = self.nodes[index];
            chain.push((index, node));
            next = node.parent;
        }
        for (index, node) in chain.into_iter().rev() {
            nodes.push((index, node, links));
            links = Some(links.map_or(0, |count| count + 1));
        }
        links
    }

    pub(super) fn plan(&self, parent: Parent, join: Option<TraceHandle>, shallow: bool) -> Plan {
        let mut nodes = Vec::new();
        let mut depth_cut = self
            .node(parent)
            .is_some_and(|node| node.depth >= self.max_depth);
        let links = self.plan_chain(parent, self.max_depth, shallow, &mut nodes, &mut depth_cut);
        let (parent_path, status) = Self::parent_wire(parent, links.is_some());
        let join = join.and_then(|handle| {
            let joined = Parent::Path(handle);
            depth_cut |= self
                .node(joined)
                .is_some_and(|node| node.depth > self.max_depth);
            self.plan_chain(
                joined,
                self.max_depth + 1,
                shallow,
                &mut nodes,
                &mut depth_cut,
            )
            .map(|_| Self::wire_id(handle.index() as usize))
        });
        let depth = match parent {
            Parent::Root => Some(0),
            _ => self.node(parent).map(|node| node.depth.saturating_add(1)),
        };
        Plan {
            parent: parent_path,
            status,
            depth,
            depth_cut,
            join,
            nodes,
        }
    }

    pub(super) fn write(&self, group: &mut Group<'_>, plan: &Plan, target_requested: bool) {
        for &(index, node, parent_links) in &plan.nodes {
            let (parent, status) = Self::parent_wire(node.parent, parent_links.is_some());
            group.line(path(
                group.row,
                Self::wire_id(index),
                parent,
                status,
                Some(node.depth),
                "intermediate",
                None,
                "not_applicable",
                None,
                None,
                "not_applicable",
                path_target(
                    if target_requested {
                        "unknown"
                    } else {
                        "not_requested"
                    },
                    vec![],
                    vec![],
                ),
            ));
        }
    }

    pub(super) fn commit(&mut self, plan: Plan) {
        self.retained.extend(
            plan.nodes
                .into_iter()
                .map(|(index, _, parent_links)| (index, parent_links.map_or(0, |count| count + 1))),
        );
    }
}

pub(super) fn path_target(relation: &str, bound: Vec<Value>, incompatible: Vec<usize>) -> Value {
    json!({"relation": relation, "boundOccurrences": bound, "incompatibleOccurrences": incompatible})
}

#[allow(clippy::too_many_arguments)]
pub(super) fn path(
    row: usize,
    id: usize,
    parent: Value,
    parent_status: &str,
    depth: Option<usize>,
    outcome: &str,
    output_id: Option<usize>,
    output_status: &str,
    first_reject: Option<usize>,
    merge_id: Option<usize>,
    merge_status: &str,
    target: Value,
) -> Value {
    json!({"type": "path", "row": row, "pathId": id, "parentPathId": parent,
        "parentStatus": parent_status, "depth": depth, "outcome": outcome,
        "outputId": output_id, "outputStatus": output_status,
        "firstRejectEventId": first_reject, "mergedIntoPathId": merge_id,
        "mergeStatus": merge_status, "target": target})
}
