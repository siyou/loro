use crate::{id::Counter, ContentType, InsertContent, ID};
use rle::{rle_tree::tree_trait::CumulateTreeTrait, HasLength, Mergable, Sliceable};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Status {
    unapplied: bool,
    delete_times: usize,
    undo_times: usize,
}

impl Status {
    #[inline]
    pub fn new() -> Self {
        Status {
            unapplied: false,
            delete_times: 0,
            undo_times: 0,
        }
    }

    #[inline]
    pub fn is_activated(&self) -> bool {
        !self.unapplied && self.delete_times == 0 && self.undo_times == 0
    }

    /// Return whether the activation changed
    #[inline]
    pub fn apply(&mut self, change: StatusChange) -> bool {
        let activated = self.is_activated();
        match change {
            StatusChange::Apply => self.unapplied = false,
            StatusChange::PreApply => self.unapplied = true,
            StatusChange::Redo => self.undo_times -= 1,
            StatusChange::Undo => self.undo_times += 1,
            StatusChange::Delete => self.delete_times += 1,
            StatusChange::UndoDelete => self.delete_times -= 1,
        }

        self.is_activated() != activated
    }
}

#[derive(Debug, Clone)]
pub(super) struct YSpan {
    pub origin_left: Option<ID>,
    pub origin_right: Option<ID>,
    pub id: ID,
    pub len: usize,
    pub status: Status,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum StatusChange {
    Apply,
    PreApply,
    Redo,
    Undo,
    Delete,
    UndoDelete,
}

pub(super) type YSpanTreeTrait = CumulateTreeTrait<YSpan, 10>;

impl YSpan {
    #[inline]
    pub fn last_id(&self) -> ID {
        self.id.inc(self.len as i32 - 1)
    }

    #[inline]
    pub fn can_be_origin(&self) -> bool {
        debug_assert!(self.len > 0);
        self.status.is_activated()
    }

    #[inline]
    pub fn contain_id(&self, id: ID) -> bool {
        self.id.client_id == id.client_id
            && self.id.counter <= id.counter
            && self.last_id().counter > id.counter
    }
}

impl Mergable for YSpan {
    fn is_mergable(&self, other: &Self, _: &()) -> bool {
        other.id.client_id == self.id.client_id
            && self.id.counter + self.len() as Counter == other.id.counter
            && Some(self.id.inc(self.len as Counter - 1)) == other.origin_left
            && self.origin_right == other.origin_right
            && self.status == other.status
    }

    fn merge(&mut self, other: &Self, _: &()) {
        self.origin_right = other.origin_right;
        self.len += other.len;
    }
}

impl Sliceable for YSpan {
    fn slice(&self, from: usize, to: usize) -> Self {
        if from == 0 {
            YSpan {
                origin_left: self.origin_left,
                origin_right: self.origin_right,
                id: self.id,
                len: to - from,
                status: self.status.clone(),
            }
        } else {
            YSpan {
                origin_left: Some(ID {
                    client_id: self.id.client_id,
                    counter: self.id.counter + from as Counter - 1,
                }),
                origin_right: self.origin_right,
                id: ID {
                    client_id: self.id.client_id,
                    counter: self.id.counter + from as Counter,
                },
                len: to - from,
                status: self.status.clone(),
            }
        }
    }
}

impl InsertContent for YSpan {
    fn id(&self) -> ContentType {
        ContentType::Text
    }
}

impl HasLength for YSpan {
    #[inline]
    fn len(&self) -> usize {
        if self.status.is_activated() {
            self.len
        } else {
            0
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        container::{ContainerID, ContainerType},
        id::ROOT_ID,
        ContentType, Op, OpContent, ID,
    };
    use rle::{HasLength, RleVec};

    use super::YSpan;

    #[test]
    fn test_merge() {
        let mut vec: RleVec<Op> = RleVec::new();
        vec.push(Op::new(
            ID::new(0, 1),
            OpContent::Normal {
                content: Box::new(YSpan {
                    origin_left: Some(ID::new(0, 0)),
                    origin_right: None,
                    id: ID::new(0, 1),
                    len: 1,
                    status: Default::default(),
                }),
            },
            ContainerID::Normal {
                id: ROOT_ID,
                container_type: ContainerType::Text,
            },
        ));
        vec.push(Op::new(
            ID::new(0, 2),
            OpContent::Normal {
                content: Box::new(YSpan {
                    origin_left: Some(ID::new(0, 1)),
                    origin_right: None,
                    id: ID::new(0, 2),
                    len: 1,
                    status: Default::default(),
                }),
            },
            ContainerID::Normal {
                id: ROOT_ID,
                container_type: ContainerType::Text,
            },
        ));
        assert_eq!(vec.merged_len(), 1);
        let merged = vec.get_merged(0).unwrap();
        assert_eq!(merged.insert_content().id(), ContentType::Text);
        let text_content =
            crate::op::utils::downcast_ref::<YSpan>(&**merged.insert_content()).unwrap();
        assert_eq!(text_content.len(), 2);
    }

    #[test]
    fn slice() {
        let mut vec: RleVec<Op> = RleVec::new();
        vec.push(Op::new(
            ID::new(0, 1),
            OpContent::Normal {
                content: Box::new(YSpan {
                    origin_left: Some(ID::new(0, 0)),
                    origin_right: None,
                    id: ID::new(0, 1),
                    len: 4,
                    status: Default::default(),
                }),
            },
            ContainerID::Normal {
                id: ROOT_ID,
                container_type: ContainerType::Text,
            },
        ));
        vec.push(Op::new(
            ID::new(0, 2),
            OpContent::Normal {
                content: Box::new(YSpan {
                    origin_left: Some(ID::new(0, 0)),
                    origin_right: Some(ID::new(0, 1)),
                    id: ID::new(0, 5),
                    len: 4,
                    status: Default::default(),
                }),
            },
            ContainerID::Normal {
                id: ROOT_ID,
                container_type: ContainerType::Text,
            },
        ));
        assert_eq!(vec.merged_len(), 2);
        assert_eq!(
            vec.slice_iter(2, 6)
                .map(|x| crate::op::utils::downcast_ref::<YSpan>(
                    &**x.into_inner().insert_content()
                )
                .unwrap()
                .len)
                .collect::<Vec<usize>>(),
            vec![2, 2]
        )
    }
}