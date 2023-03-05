use enum_as_inner::EnumAsInner;
use rle::{HasLength, Mergable, Sliceable};
use serde::Serialize;
use smallvec::{smallvec, SmallVec};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqDelta<Value, Meta = ()> {
    pub(crate) vec: SmallVec<[DeltaItem<Value, Meta>; 2]>,
}

impl<V: Serialize, M: Serialize> Serialize for SeqDelta<V, M> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.vec.iter())
    }
}

#[derive(Debug, EnumAsInner, PartialEq, Eq, Clone, Serialize)]
pub enum DeltaItem<Value, Meta> {
    Retain { len: usize, meta: Meta },
    Insert { value: Value, meta: Meta },
    Delete(usize),
}

pub trait Meta: Debug + Clone + PartialEq {
    fn none() -> Self;
    fn empty() -> Self;
    fn is_empty(&self) -> bool;
    fn is_meta_none(&self) -> bool;
    fn compose(self, other: Self) -> Self;
}

impl Meta for () {
    fn none() -> Self {}
    fn empty() -> Self {}
    fn is_empty(&self) -> bool {
        true
    }
    fn is_meta_none(&self) -> bool {
        true
    }
    fn compose(self, _other: Self) -> Self {}
}

pub trait DeltaValue: Debug + HasLength + Sliceable + Clone + PartialEq {
    fn extend(&mut self, other: Self);
}

impl<Value: DeltaValue, M: Meta> DeltaItem<Value, M> {
    pub fn get_meta(&self) -> Option<&M> {
        match self {
            DeltaItem::Insert { meta, .. } => Some(meta),
            DeltaItem::Retain { meta, .. } => Some(meta),
            _ => None,
        }
    }

    pub fn meta(&mut self, meta: M) {
        match self {
            DeltaItem::Insert { meta: m, .. } => *m = meta,
            DeltaItem::Retain { meta: m, .. } => *m = meta,
            _ => {}
        }
    }

    pub fn is_retain(&self) -> bool {
        matches!(self, Self::Retain { .. })
    }

    pub fn is_insert(&self) -> bool {
        matches!(self, Self::Insert { .. })
    }

    pub fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }
}

impl<Value: HasLength, Meta> HasLength for DeltaItem<Value, Meta> {
    fn content_len(&self) -> usize {
        match self {
            DeltaItem::Retain { len, meta: _ } => *len,
            DeltaItem::Insert { value, meta: _ } => value.atom_len(),
            DeltaItem::Delete(len) => *len,
        }
    }
}

impl<Value, Meta> Mergable for DeltaItem<Value, Meta> {}

pub struct DeltaIterator<V, M: Meta> {
    ops: SmallVec<[DeltaItem<V, M>; 2]>,
    index: usize,
    offset: usize,
}

impl<V: DeltaValue, M: Meta> DeltaIterator<V, M> {
    fn new(ops: SmallVec<[DeltaItem<V, M>; 2]>) -> Self {
        Self {
            ops,
            index: 0,
            offset: 0,
        }
    }

    fn next<L: Into<Option<usize>>>(&mut self, len: L) -> DeltaItem<V, M> {
        self.next_impl(len.into())
    }

    fn next_impl(&mut self, mut len: Option<usize>) -> DeltaItem<V, M> {
        if len.is_none() {
            len = Some(usize::MAX)
        }
        let mut length = len.unwrap();
        {
            let next_op = self.peek();
            if next_op.is_none() {
                return DeltaItem::Retain {
                    len: usize::MAX,
                    meta: M::none(),
                };
            }
        }
        // TODO: Maybe can avoid cloning
        let op = self.peek().unwrap();
        let op_length = op.content_len();
        let offset = self.offset;
        let (index_delta, offset_delta) = {
            if length >= op_length - offset {
                length = op_length - offset;
                (1, -(offset as isize))
            } else {
                (0, length as isize)
            }
        };

        let ans = if op.is_delete() {
            DeltaItem::Delete(length)
        } else {
            let mut ans_op = op.clone();
            if ans_op.is_retain() {
                *ans_op.as_retain_mut().unwrap().0 = length;
            } else if ans_op.is_insert() {
                let v = ans_op.as_insert_mut().unwrap().0;
                *v = v.slice(offset, offset + length);
            }
            ans_op
        };

        self.index += index_delta;
        self.offset = (self.offset as isize + offset_delta) as usize;
        ans
    }

    fn rest(&mut self) -> SmallVec<[DeltaItem<V, M>; 2]> {
        if !self.has_next() {
            smallvec![]
        } else if self.offset == 0 {
            // TODO avoid cloning
            self.ops[self.index..].into()
        } else {
            let offset = self.offset;
            let index = self.index;
            let next = self.next(None);
            let rest = self.ops[self.index..].to_vec();
            self.offset = offset;
            self.index = index;
            let mut ans = smallvec![next];
            ans.extend(rest);
            ans
        }
    }

    fn has_next(&self) -> bool {
        self.peek_length() < usize::MAX
    }

    fn peek(&self) -> Option<&DeltaItem<V, M>> {
        self.ops.get(self.index)
    }

    fn peek_length(&self) -> usize {
        if let Some(op) = self.peek() {
            if op.content_len() == usize::MAX {
                return usize::MAX;
            }
            op.content_len() - self.offset
        } else {
            usize::MAX
        }
    }

    // fn peek_is_retain(&self) -> bool {
    //     if let Some(op) = self.peek() {
    //         op.is_retain()
    //     } else {
    //         // default
    //         true
    //     }
    // }

    fn peek_is_insert(&self) -> bool {
        if let Some(op) = self.peek() {
            op.is_insert()
        } else {
            false
        }
    }

    fn peek_is_delete(&self) -> bool {
        if let Some(op) = self.peek() {
            op.is_delete()
        } else {
            false
        }
    }
}

impl<Value: DeltaValue, M: Meta> SeqDelta<Value, M> {
    pub fn new() -> Self {
        Self {
            vec: SmallVec::new(),
        }
    }

    pub fn items(&self) -> &[DeltaItem<Value, M>] {
        &self.vec
    }

    pub fn inner(self) -> SmallVec<[DeltaItem<Value, M>; 2]> {
        self.vec
    }

    pub fn retain_with_meta(mut self, len: usize, meta: M) -> Self {
        self.push(DeltaItem::Retain { len, meta });
        self
    }

    pub fn insert_with_meta<V: Into<Value>>(mut self, value: V, meta: M) -> Self {
        self.push(DeltaItem::Insert {
            value: value.into(),
            meta,
        });
        self
    }

    pub fn delete(mut self, len: usize) -> Self {
        if len == 0 {
            return self;
        }
        self.push(DeltaItem::Delete(len));
        self
    }
    pub fn retain(mut self, len: usize) -> Self {
        if len == 0 {
            return self;
        }

        self.push(DeltaItem::Retain {
            len,
            meta: M::none(),
        });
        self
    }

    pub fn insert<V: Into<Value>>(mut self, value: V) -> Self {
        self.push(DeltaItem::Insert {
            value: value.into(),
            meta: M::none(),
        });
        self
    }

    pub fn push(&mut self, new_op: DeltaItem<Value, M>) {
        let mut index = self.vec.len();
        let last_op = self.vec.last_mut();
        if let Some(mut last_op) = last_op {
            if new_op.is_delete() && last_op.is_delete() {
                self.vec[index - 1] =
                    DeltaItem::Delete(last_op.content_len() + new_op.content_len());
                return;
            }
            // Since it does not matter if we insert before or after deleting at the same index,
            // always prefer to insert first
            if last_op.is_delete() && new_op.is_insert() {
                index -= 1;
                if index == 0 {
                    self.vec.insert(0, new_op);
                    return;
                }
                let _last_op = self.vec.get_mut(index - 1);
                if let Some(_last_op_inner) = _last_op {
                    last_op = _last_op_inner;
                } else {
                    self.vec.insert(0, new_op);
                    return;
                }
            }
            if new_op.get_meta() == last_op.get_meta() {
                if new_op.is_insert() && last_op.is_insert() {
                    // TODO avoid cloning
                    let mut value = last_op.as_insert_mut().unwrap().0.clone();
                    value.extend(new_op.as_insert().unwrap().0.clone());
                    self.vec[index - 1] = DeltaItem::Insert {
                        value,
                        meta: new_op.get_meta().unwrap().clone(),
                    };
                    return;
                } else if new_op.is_retain() && last_op.is_retain() {
                    self.vec[index - 1] = DeltaItem::Retain {
                        len: last_op.content_len() + new_op.content_len(),
                        meta: new_op.get_meta().unwrap().clone(),
                    };
                    return;
                }
            }
        }
        if index == self.vec.len() {
            self.vec.push(new_op);
        } else {
            self.vec.insert(index, new_op);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeltaItem<Value, M>> {
        self.vec.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut DeltaItem<Value, M>> {
        self.vec.iter_mut()
    }

    pub fn into_op_iter(self) -> DeltaIterator<Value, M> {
        DeltaIterator::new(self.vec)
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reference: [Quill Delta](https://github.com/quilljs/delta)
    pub fn compose(self, other: Self) -> Self {
        let mut this_iter = self.into_op_iter();
        let mut other_iter = other.into_op_iter();
        let mut ops = smallvec![];
        let first_other = other_iter.peek();
        if let Some(first_other) = first_other {
            if first_other.is_retain() && first_other.get_meta().is_none() {
                let mut first_left = first_other.content_len();
                let mut first_this = this_iter.peek();
                while let Some(first_this_inner) = first_this {
                    if first_this_inner.is_insert() && first_this_inner.content_len() <= first_left
                    {
                        first_left -= first_this_inner.content_len();
                        ops.push(this_iter.next(None));
                        first_this = this_iter.peek();
                    } else {
                        break;
                    }
                }
                if first_other.content_len() - first_left > 0 {
                    other_iter.next(first_other.content_len() - first_left);
                }
            }
        }
        let mut delta = SeqDelta { vec: ops };
        while this_iter.has_next() || other_iter.has_next() {
            if other_iter.peek_is_insert() {
                delta.push(other_iter.next(None));
            } else if this_iter.peek_is_delete() {
                delta.push(this_iter.next(None));
            } else {
                let length = this_iter.peek_length().min(other_iter.peek_length());
                let this_op = this_iter.next(length);
                let other_op = other_iter.next(length);
                if other_op.is_retain() {
                    let mut new_op = if this_op.is_retain() {
                        DeltaItem::Retain {
                            len: length,
                            meta: M::none(),
                        }
                    } else {
                        this_op.clone()
                    };
                    let meta = this_op
                        .get_meta()
                        .unwrap()
                        .clone()
                        .compose(other_op.get_meta().unwrap().clone());
                    new_op.meta(meta);
                    delta.push(new_op.clone());
                    if !other_iter.has_next() && delta.vec.last().unwrap().eq(&new_op) {
                        let vec = this_iter.rest();
                        if vec.is_empty() {
                            return delta.chop();
                        }
                        let rest = SeqDelta { vec };
                        return delta.concat(rest).chop();
                    }
                } else if other_op.is_delete() && this_op.is_retain() {
                    delta.push(other_op);
                }
            }
        }
        delta.chop()
    }

    fn concat(&mut self, mut other: Self) -> Self {
        let mut delta = SeqDelta {
            vec: self.vec.clone(),
        };
        if !other.vec.is_empty() {
            // TODO: why?
            let other_first = other.vec.remove(0);
            delta.push(other_first);
            delta.vec.extend(other.vec);
        }
        delta
    }

    fn chop(mut self) -> Self {
        let last_op = self.vec.last();
        if let Some(last_op) = last_op {
            if last_op.is_retain() && last_op.get_meta().unwrap().is_meta_none() {
                self.vec.pop();
            }
        }
        self
    }
}

impl<Value: DeltaValue, M: Meta> Default for SeqDelta<Value, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + PartialEq + Debug> DeltaValue for Vec<T> {
    fn extend(&mut self, other: Self) {
        <Vec<_> as std::iter::Extend<_>>::extend(self, other)
    }
}

impl DeltaValue for String {
    fn extend(&mut self, other: Self) {
        self.push_str(&other)
    }
}

#[cfg(test)]
mod test {
    use super::{DeltaItem, Meta, SeqDelta};

    #[derive(Debug, PartialEq, Clone, Default)]
    struct M {
        bold: Option<bool>,
        color: Option<()>,
    }

    type TestMeta = Option<M>;

    impl Meta for TestMeta {
        fn none() -> Self {
            None
        }

        fn is_meta_none(&self) -> bool {
            self.is_none()
        }

        fn empty() -> Self {
            Some(M {
                bold: None,
                color: None,
            })
        }
        fn is_empty(&self) -> bool {
            self.is_some()
                && self.as_ref().unwrap().bold.is_none()
                && self.as_ref().unwrap().color.is_none()
        }

        fn compose(self, other: Self) -> Self {
            if other.is_empty() {
                None
            } else if other.is_none() {
                self
            } else {
                other
            }
        }
    }

    type TestDelta = SeqDelta<String, TestMeta>;
    const BOLD_META: TestMeta = Some(M {
        bold: Some(true),
        color: None,
    });
    const COLOR_META: TestMeta = Some(M {
        bold: None,
        color: Some(()),
    });
    const EMPTY_META: TestMeta = Some(M {
        bold: None,
        color: None,
    });
    // const ALL_META: TestMeta = Some(M {
    //     bold: Some(true),
    //     color: Some(()),
    // });

    #[test]
    fn delta_push() {
        let mut a: SeqDelta<String, ()> = SeqDelta::new().insert("a".to_string());
        a.push(DeltaItem::Insert {
            value: "b".to_string(),
            meta: (),
        });
        assert_eq!(a, SeqDelta::new().insert("ab".to_string()));
    }

    #[test]
    fn delta_compose() {
        let a: SeqDelta<String, ()> = SeqDelta::new().retain(3).insert("abcde".to_string());
        let b = SeqDelta::new().retain(5).delete(6);
        assert_eq!(
            a.compose(b),
            SeqDelta::new().retain(3).insert("ab".to_string()).delete(3)
        );

        let a: SeqDelta<String, ()> = SeqDelta::new().insert("123".to_string());
        let b = SeqDelta::new().retain(1).insert("123".to_string());
        assert_eq!(a.compose(b), SeqDelta::new().insert("112323".to_string()));
    }

    #[test]
    fn insert_insert() {
        let a = TestDelta::new().insert("a");
        let b = TestDelta::new().insert("b");
        assert_eq!(a.compose(b), TestDelta::new().insert("b").insert("a"));
    }

    #[test]
    fn insert_retain() {
        let a = TestDelta::new().insert("a");
        let b = TestDelta::new().retain_with_meta(1, BOLD_META);
        assert_eq!(
            a.compose(b),
            TestDelta::new().insert_with_meta("a", BOLD_META)
        );
    }

    #[test]
    fn insert_delete() {
        let a = TestDelta::new().insert("a");
        let b = TestDelta::new().delete(1);
        assert_eq!(a.compose(b), TestDelta::new());
    }

    #[test]
    fn delete_insert() {
        let a = TestDelta::new().delete(1);
        let b = TestDelta::new().insert("b");
        assert_eq!(a.compose(b), TestDelta::new().insert("b").delete(1));
    }

    #[test]
    fn delete_retain() {
        let a = TestDelta::new().delete(1);
        let b = TestDelta::new().retain_with_meta(1, BOLD_META);
        assert_eq!(
            a.compose(b),
            TestDelta::new().delete(1).retain_with_meta(1, BOLD_META)
        );
    }

    #[test]
    fn delete_delete() {
        let a = TestDelta::new().delete(1);
        let b = TestDelta::new().delete(1);
        assert_eq!(a.compose(b), TestDelta::new().delete(2));
    }

    #[test]
    fn retain_insert() {
        let a = TestDelta::new().retain_with_meta(1, BOLD_META);
        let b = TestDelta::new().insert("b");
        assert_eq!(
            a.compose(b),
            TestDelta::new().insert("b").retain_with_meta(1, BOLD_META)
        );
    }

    #[test]
    fn retain_retain() {
        let a = TestDelta::new().retain_with_meta(1, BOLD_META);
        let b = TestDelta::new().retain_with_meta(1, COLOR_META);
        assert_eq!(
            a.compose(b),
            TestDelta::new().retain_with_meta(1, COLOR_META)
        );
    }

    #[test]
    fn retain_delete() {
        let a = TestDelta::new().retain_with_meta(1, BOLD_META);
        let b = TestDelta::new().delete(1);
        assert_eq!(a.compose(b), TestDelta::new().delete(1));
    }

    #[test]
    fn delete_entire() {
        let a = TestDelta::new().retain(4).insert("abcde");
        let b = TestDelta::new().delete(9);
        assert_eq!(a.compose(b), TestDelta::new().delete(4));
    }

    #[test]
    fn retain_more() {
        let a = TestDelta::new().insert("abcde");
        let b = TestDelta::new().retain(10);
        assert_eq!(a.compose(b), TestDelta::new().insert("abcde"));
    }

    #[test]
    fn remove_meta() {
        let a = TestDelta::new().insert_with_meta("a", BOLD_META);
        let b = TestDelta::new().retain_with_meta(1, EMPTY_META);
        assert_eq!(a.compose(b), TestDelta::new().insert("a"));
    }

    #[test]
    fn retain_start_opt() {
        let a = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META)
            .delete(1);
        let b = TestDelta::new().retain(3).insert("d");
        let expect = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META)
            .insert("d")
            .delete(1);
        assert_eq!(a.compose(b), expect);
    }

    #[test]
    fn retain_start_opt_split() {
        let a = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META)
            .retain(5)
            .delete(1);
        let b = TestDelta::new().retain(4).insert("d");
        let expect = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META)
            .retain(1)
            .insert("d")
            .retain(4)
            .delete(1);
        assert_eq!(a.compose(b), expect);
    }

    #[test]
    fn retain_end_opt() {
        let a = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META);
        let b = TestDelta::new().delete(1);
        let expect = TestDelta::new()
            .insert("b")
            .insert_with_meta("c", BOLD_META);
        assert_eq!(a.compose(b), expect);
    }

    #[test]
    fn retain_end_opt_join() {
        let a = TestDelta::new()
            .insert_with_meta("a", BOLD_META)
            .insert("b")
            .insert_with_meta("c", BOLD_META)
            .insert("d")
            .insert_with_meta("e", BOLD_META)
            .insert("f");
        let b = TestDelta::new().retain(1).delete(1);
        let expect = TestDelta::new()
            .insert_with_meta("ac", BOLD_META)
            .insert("d")
            .insert_with_meta("e", BOLD_META)
            .insert("f");
        assert_eq!(a.compose(b), expect);
    }
}
