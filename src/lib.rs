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

impl<C> Annotation<LinkedList<C, Cardinality>> for Cardinality {
    fn from_child(list: &LinkedList<C, Cardinality>) -> Self {
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

struct LinkedList<C, A> {
    elem: Option<C>,
    // placing a reference type wrapped by Annotated is the easiest way to
    // keep annotations with your data.
    next: Option<Annotated<Rc<LinkedList<C, A>>, A>>,
}

impl<C, A> LinkedList<C, A>
where
    A: Annotation<LinkedList<C, A>>,
{
    fn new() -> Self {
        Self {
            elem: None,
            next: None,
        }
    }

    fn push(&mut self, data: C) {
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

    fn pop(&mut self) -> Option<C> {
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
#![no_std]
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
pub struct Annotated<C, A> {
    child: C,
    anno: RefCell<Option<A>>,
}

impl<C, A> Annotated<C, A> {
    /// Returns the annotation over the child.
    pub fn child(&self) -> &C {
        &self.child
    }

    /// Consume the structure and return the child and the annotation, if it
    /// was already computed.
    pub fn split(self) -> (C, Option<A>) {
        (self.child, self.anno.take())
    }
}

impl<C, A> Annotated<C, A>
where
    A: Annotation<C>,
{
    /// Create a new annotation over a child.
    pub fn new(child: C) -> Self {
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
    pub fn child_mut(&mut self) -> AnnotatedRefMut<C, A> {
        AnnotatedRefMut { annotated: self }
    }
}

impl<C, A> Default for Annotated<C, A>
where
    C: Default,
    A: Annotation<C>,
{
    fn default() -> Self {
        let elem = C::default();
        Self::new(elem)
    }
}

impl<C, A> Clone for Annotated<C, A>
where
    C: Clone,
    A: Annotation<C>,
{
    fn clone(&self) -> Self {
        let child = self.child.clone();
        Self::new(child)
    }
}

impl<C, A> PartialEq for Annotated<C, A>
where
    C: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.child, &other.child)
    }
}

impl<C, A> Eq for Annotated<C, A> where C: PartialEq + Eq {}

impl<C, A> PartialOrd for Annotated<C, A>
where
    C: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&self.child, &other.child)
    }
}

impl<C, A> Ord for Annotated<C, A>
where
    C: PartialOrd + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.child, &other.child)
    }
}

impl<C, A> From<C> for Annotated<C, A>
where
    A: Annotation<C>,
{
    fn from(elem: C) -> Self {
        Self::new(elem)
    }
}

/// A mutable reference to an annotated child.
///
/// If the value is mutably de-referenced, the annotation is invalidated and
/// will need to be re-computed.
#[derive(Debug)]
pub struct AnnotatedRefMut<'a, C, A> {
    annotated: &'a mut Annotated<C, A>,
}

impl<'a, C, A> Deref for AnnotatedRefMut<'a, C, A> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.annotated.child
    }
}

impl<'a, C, A> DerefMut for AnnotatedRefMut<'a, C, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // when de-referencing mutably, invalidate the annotation
        self.annotated.anno = RefCell::new(None);

        &mut self.annotated.child
    }
}

/// Annotation over a child.
pub trait Annotation<C> {
    /// Compute the annotation from the child.
    fn from_child(t: &C) -> Self;
}

impl<'a, C, A> Annotation<&'a C> for A
where
    A: Annotation<C>,
{
    fn from_child(t: &&'a C) -> Self {
        A::from_child(t)
    }
}

impl<'a, C, A> Annotation<&'a mut C> for A
where
    A: Annotation<C>,
{
    fn from_child(t: &&'a mut C) -> Self {
        A::from_child(t)
    }
}

#[cfg(feature = "alloc")]
mod impl_alloc {
    use super::Annotation;

    extern crate alloc;

    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::sync::Arc;

    impl<C, A> Annotation<Rc<C>> for A
    where
        A: Annotation<C>,
    {
        fn from_child(t: &Rc<C>) -> Self {
            A::from_child(t.as_ref())
        }
    }

    impl<C, A> Annotation<Arc<C>> for A
    where
        A: Annotation<C>,
    {
        fn from_child(t: &Arc<C>) -> Self {
            A::from_child(t.as_ref())
        }
    }

    impl<C, A> Annotation<Box<C>> for A
    where
        A: Annotation<C>,
    {
        fn from_child(t: &Box<C>) -> Self {
            A::from_child(t.as_ref())
        }
    }
}
