//! Split View / Groups system - Supports vertical and horizontal splits with resizable proportions.
//! Supports vertical and horizontal splits with resizable proportions.

/// Unique ID for a split group
pub type GroupId = u64;

static mut NEXT_GROUP_ID: GroupId = 0;

fn next_group_id() -> GroupId {
    unsafe {
        let id = NEXT_GROUP_ID;
        NEXT_GROUP_ID += 1;
        id
    }
}

/// Layout direction for a split
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Splits left-to-right (vertical divider)
    Vertical,
    /// Splits top-to-bottom (horizontal divider)
    Horizontal,
}

/// A split group - a single editor area that can hold tabs
#[derive(Debug, Clone)]
pub struct SplitGroup {
    pub id: GroupId,
    pub active_tab: Option<usize>,
}

impl SplitGroup {
    pub fn new() -> Self {
        Self {
            id: next_group_id(),
            active_tab: None,
        }
    }
}

/// A split node - either a single group or a split containing two nodes
#[derive(Debug, Clone)]
pub enum SplitNode {
    /// Single group (no split)
    Group(GroupId),
    /// Split between two nodes
    Split {
        direction: SplitDirection,
        left_or_top: Box<SplitNode>,
        right_or_bottom: Box<SplitNode>,
        /// Proportion [0.0, 1.0] - how much of the space the left/top side gets
        proportion: f32,
    },
}

impl SplitNode {
    /// Create a new single-group node
    pub fn new_group(id: GroupId) -> Self {
        SplitNode::Group(id)
    }

    /// Recursively extract all group IDs
    pub fn group_ids(&self, out: &mut Vec<GroupId>) {
        match self {
            SplitNode::Group(id) => out.push(*id),
            SplitNode::Split {
                left_or_top,
                right_or_bottom,
                ..
            } => {
                left_or_top.group_ids(out);
                right_or_bottom.group_ids(out);
            }
        }
    }

    /// Get the proportion of this node (1.0 for single group, stored proportion for split)
    pub fn proportion(&self) -> f32 {
        match self {
            SplitNode::Group(_) => 1.0,
            SplitNode::Split { proportion, .. } => *proportion,
        }
    }
}

/// Split view manager - handles groups and splits
#[derive(Debug)]
pub struct SplitView {
    /// The root split node
    root: SplitNode,
    /// Maps group IDs to their SplitGroups
    groups: std::collections::HashMap<GroupId, SplitGroup>,
}

impl SplitView {
    pub fn new() -> Self {
        let id = next_group_id();
        let root = SplitNode::new_group(id);
        let mut groups = std::collections::HashMap::new();
        groups.insert(id, SplitGroup::new());

        Self {
            root,
            groups,
        }
    }

    /// Get the root node
    pub fn root(&self) -> &SplitNode {
        &self.root
    }

    /// Split the active group vertically (side by side)
    /// Returns the new active group (the right side)
    pub fn split_active_vertical(&mut self, active_group: GroupId) -> GroupId {
        let new_group = SplitGroup::new();
        let new_id = new_group.id;
        self.groups.insert(new_id, new_group);

        // Replace the active group in the root with a vertical split
        let mut make_split = |old_id: GroupId, replacement: SplitNode| {
            // Create a vertical split: old on left, new on right
            SplitNode::Split {
                direction: SplitDirection::Vertical,
                left_or_top: Box::new(SplitNode::Group(old_id)),
                right_or_bottom: Box::new(replacement),
                proportion: 0.5,
            }
        };
        let new_node = Self::replace_group_in_tree(
            &self.root,
            active_group,
            SplitNode::Group(new_id),
            &mut make_split,
        );

        self.root = new_node;
        new_id
    }

    /// Split the active group horizontally (stacked)
    pub fn split_active_horizontal(&mut self, active_group: GroupId) -> GroupId {
        let new_group = SplitGroup::new();
        let new_id = new_group.id;
        self.groups.insert(new_id, new_group);

        let mut make_split = |old_id: GroupId, replacement: SplitNode| {
            // Create a horizontal split: old on top, new on bottom
            SplitNode::Split {
                direction: SplitDirection::Horizontal,
                left_or_top: Box::new(SplitNode::Group(old_id)),
                right_or_bottom: Box::new(replacement),
                proportion: 0.5,
            }
        };
        let new_node = Self::replace_group_in_tree(
            &self.root,
            active_group,
            SplitNode::Group(new_id),
            &mut make_split,
        );

        self.root = new_node;
        new_id
    }

    /// Replace a group ID in the tree with a new node, returning a modified tree
    fn replace_group_in_tree(
        node: &SplitNode,
        target: GroupId,
        replacement: SplitNode,
        mut make_split: &mut dyn FnMut(GroupId, SplitNode) -> SplitNode,
    ) -> SplitNode {
        match node {
            SplitNode::Group(id) if *id == target => make_split(*id, replacement),
            SplitNode::Split {
                direction,
                left_or_top,
                right_or_bottom,
                proportion,
            } => {
                let new_left = Self::replace_group_in_tree(left_or_top, target, replacement.clone(), make_split);
                let new_right = Self::replace_group_in_tree(right_or_bottom, target, replacement, make_split);

                match (&new_left, &new_right) {
                    (SplitNode::Group(left_id), SplitNode::Group(_)) => {
                        make_split(*left_id, new_right)
                    }
                    _ => SplitNode::Split {
                        direction: *direction,
                        left_or_top: Box::new(new_left),
                        right_or_bottom: Box::new(new_right),
                        proportion: *proportion,
                    },
                }
            }
            other => other.clone(),
        }
    }

    /// Get or create a group for a given ID
    pub fn ensure_group(&mut self, id: GroupId) -> &mut SplitGroup {
        self.groups.entry(id).or_insert_with(SplitGroup::new)
    }

    /// Get a group by ID
    pub fn group(&self, id: GroupId) -> Option<&SplitGroup> {
        self.groups.get(&id)
    }

    /// Get mutable reference to a group
    pub fn group_mut(&mut self, id: GroupId) -> Option<&mut SplitGroup> {
        self.groups.get_mut(&id)
    }

    /// Get all group IDs in the current layout
    pub fn all_group_ids(&self) -> Vec<GroupId> {
        let mut ids = Vec::new();
        self.root.group_ids(&mut ids);
        ids
    }

    /// Check if there's only one group (no splits)
    pub fn is_single_group(&self) -> bool {
        matches!(&self.root, SplitNode::Group(_))
    }

    /// Reset to a single group
    pub fn reset(&mut self) {
        let single_group = SplitGroup::new();
        let id = single_group.id;
        self.groups.clear();
        self.groups.insert(id, single_group);
        self.root = SplitNode::Group(id);
    }
}
