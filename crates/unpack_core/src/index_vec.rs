use std::{marker::PhantomData, ops};

pub(crate) trait Idx: Copy {
    fn from_usize(index: usize) -> Self;
    fn index(self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexVec<I, T> {
    raw: Vec<T>,
    _index: PhantomData<fn(I) -> I>,
}

impl<I, T> IndexVec<I, T>
where
    I: Idx,
{
    pub(crate) fn push(&mut self, value: T) -> I {
        let index = I::from_usize(self.raw.len());
        self.raw.push(value);
        index
    }

    pub(crate) fn get(&self, index: I) -> Option<&T> {
        self.raw.get(index.index())
    }
}

impl<I, T> Default for IndexVec<I, T> {
    fn default() -> Self {
        Self {
            raw: Vec::new(),
            _index: PhantomData,
        }
    }
}

impl<I, T> ops::Index<I> for IndexVec<I, T>
where
    I: Idx,
{
    type Output = T;

    fn index(&self, index: I) -> &Self::Output {
        &self.raw[index.index()]
    }
}

impl<I, T> ops::IndexMut<I> for IndexVec<I, T>
where
    I: Idx,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.raw[index.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::{Idx, IndexVec};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestIndex(usize);

    impl Idx for TestIndex {
        fn from_usize(index: usize) -> Self {
            Self(index)
        }

        fn index(self) -> usize {
            self.0
        }
    }

    #[test]
    fn assigns_and_uses_typed_indices() {
        let mut values: IndexVec<TestIndex, _> = IndexVec::default();
        let first = values.push("first");
        let second = values.push("second");

        assert_eq!(first, TestIndex(0));
        assert_eq!(second, TestIndex(1));
        assert_eq!(values[first], "first");
        assert_eq!(values.get(second), Some(&"second"));

        values[second] = "updated";
        assert_eq!(values[second], "updated");
    }
}
