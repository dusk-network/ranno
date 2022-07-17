// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![cfg_attr(
    feature = "alloc",
    doc = r##"
Annotations over recursive data structures.

An [`Annotation`] is a type that annotates a child of a recursive data
structure with some extra information. Implementing it for a type allows that
type to also be an annotation over a reference to a child.

The [`Annotated`] type is provided to compute and store the annotation over a
reference to a child. Annotations are computed lazily, triggered by when a
reference to them is asked for.

# Example
```
extern crate alloc;
use alloc::rc::Rc;

use core::mem;
use ranno::{Annotated, Annotation};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Cardinality(usize);

impl<T> Annotation<LinkedList<T, Cardinality>> for Cardinality {
    fn from_child(list: &LinkedList<T, Cardinality>) -> Self {
        let elem_card = match list.elem {
            None => 0,
            Some(_) => 1,
        };

        let next_card = match list.next {
            None => 0,
            Some(ref annotated) => annotated.anno().0,
        };

        // the cardinality of a linked list is just the cardinality of the
        // next element added to the current one
        Self(elem_card + next_card)
    }
}

struct LinkedList<T, A> {
    elem: Option<T>,
    // placing a reference type wrapped by Annotated is the easiest way to
    // keep annotations with your data.
    next: Option<Annotated<Rc<LinkedList<T, A>>, A>>,
}

impl<T, A> LinkedList<T, A>
where
    A: Annotation<LinkedList<T, A>>,
{
    fn new() -> Self {
        Self {
            elem: None,
            next: None,
        }
    }

    fn push(&mut self, data: T) {
        if self.elem.is_none() {
            self.elem = Some(data);
            return;
        }

        let mut new_list = LinkedList {
            elem: Some(data),
            next: None,
        };
        mem::swap(&mut new_list, self);

        let anno = Annotated::new(Rc::new(new_list));
        self.next = Some(anno);
    }

    fn pop(&mut self) -> Option<T> {
        if self.next.is_none() {
            return self.elem.take();
        }

        let anno = self.next.take()?;
        let (child, _) = anno.split();

        match Rc::try_unwrap(child) {
            Ok(mut list) => {
                mem::swap(&mut list, self);
                Some(list.elem.unwrap())
            }
            Err(link) => {
                self.next = Some(Annotated::new(link));
                None
            }
        }
    }
}

let mut list = LinkedList::<_, Cardinality>::new();

assert_eq!(Cardinality::from_child(&list), Cardinality(0));

list.push(1);
assert_eq!(Cardinality::from_child(&list), Cardinality(1));

list.push(2);
assert_eq!(Cardinality::from_child(&list), Cardinality(2));

list.pop();
assert_eq!(Cardinality::from_child(&list), Cardinality(1));
```
"##
)]
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::all)]
#![cfg_attr(feature = "alloc", deny(missing_docs))]

use core::cell::{Ref, RefCell};
use core::cmp::Ordering;
use core::ops::{Deref, DerefMut};

/// A child annotated with some metadata.
///
/// Annotations are lazily evaluated, with computation triggered when a
/// reference to them is asked for using [`anno`].
///
/// [`anno`]: Annotated::anno
#[derive(Debug)]
pub struct Annotated<T, A> {
    child: T,
    anno: RefCell<Option<A>>,
}

impl<T, A> Annotated<T, A> {
    /// Returns the annotation over the child.
    pub fn child(&self) -> &T {
        &self.child
    }

    /// Consume the structure and return the child and the annotation, if it
    /// was already computed.
    pub fn split(self) -> (T, Option<A>) {
        (self.child, self.anno.take())
    }
}

impl<T, A> Annotated<T, A>
where
    A: Annotation<T>,
{
    /// Create a new annotation over a child.
    pub fn new(child: T) -> Self {
        Self {
            anno: RefCell::new(None),
            child,
        }
    }

    /// Returns the annotated child.
    pub fn anno(&self) -> Ref<A> {
        // lazily compute the annotation when reference is asked for
        if self.anno.borrow().is_none() {
            let anno = A::from_child(&self.child);
            self.anno.replace(Some(anno));
        }

        // unwrapping is ok since we're sure the option is initialized
        Ref::map(self.anno.borrow(), |elem| elem.as_ref().unwrap())
    }

    /// Returns a mutable reference to the annotated child.
    pub fn child_mut(&mut self) -> AnnotatedRefMut<T, A> {
        AnnotatedRefMut { annotated: self }
    }
}

impl<T, A> Default for Annotated<T, A>
where
    T: Default,
    A: Annotation<T>,
{
    fn default() -> Self {
        let elem = T::default();
        Self::new(elem)
    }
}

impl<T, A> Clone for Annotated<T, A>
where
    T: Clone,
    A: Annotation<T>,
{
    fn clone(&self) -> Self {
        let child = self.child.clone();
        Self::new(child)
    }
}

impl<T, A> PartialEq for Annotated<T, A>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.child, &other.child)
    }
}

impl<T, A> Eq for Annotated<T, A> where T: PartialEq + Eq {}

impl<T, A> PartialOrd for Annotated<T, A>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&self.child, &other.child)
    }
}

impl<T, A> Ord for Annotated<T, A>
where
    T: PartialOrd + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.child, &other.child)
    }
}

impl<T, A> From<T> for Annotated<T, A>
where
    A: Annotation<T>,
{
    fn from(elem: T) -> Self {
        Self::new(elem)
    }
}

/// A mutable reference to an annotated child.
///
/// If the value is mutably de-referenced, the annotation is invalidated and
/// will need to be re-computed.
pub struct AnnotatedRefMut<'a, T, A> {
    annotated: &'a mut Annotated<T, A>,
}

impl<'a, T, A> Deref for AnnotatedRefMut<'a, T, A> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.annotated.child
    }
}

impl<'a, T, A> DerefMut for AnnotatedRefMut<'a, T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // when de-referencing mutably, invalidate the annotation
        self.annotated.anno = RefCell::new(None);

        &mut self.annotated.child
    }
}

/// Annotation over a child.
pub trait Annotation<T> {
    /// Compute the annotation from the child.
    fn from_child(t: &T) -> Self;
}

impl<'a, T, A> Annotation<&'a T> for A
where
    A: Annotation<T>,
{
    fn from_child(t: &&'a T) -> Self {
        A::from_child(t)
    }
}

impl<'a, T, A> Annotation<&'a mut T> for A
where
    A: Annotation<T>,
{
    fn from_child(t: &&'a mut T) -> Self {
        A::from_child(t)
    }
}

#[cfg(feature = "alloc")]
mod impl_alloc {
    use super::Annotation;

    extern crate alloc;
    use alloc::rc::Rc;

    impl<T, A> Annotation<Rc<T>> for A
    where
        A: Annotation<T>,
    {
        fn from_child(t: &Rc<T>) -> Self {
            A::from_child(t.as_ref())
        }
    }
}

#[cfg(feature = "std")]
mod impl_std {
    use super::Annotation;

    use std::sync::Arc;

    impl<T, A> Annotation<Arc<T>> for A
    where
        A: Annotation<T>,
    {
        fn from_child(t: &Arc<T>) -> Self {
            A::from_child(t.as_ref())
        }
    }
}
