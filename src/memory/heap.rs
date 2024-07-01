use core::alloc::Layout;
use core::mem;
use core::mem::{align_of, size_of};
use core::ptr::null_mut;
use core::ptr::NonNull;

/// A sorted list of holes. It uses the the holes itself to store its nodes.
pub struct HoleList {
    pub(crate) first: Hole, // dummy node
    pub(crate) bottom: *mut u8,
    pub(crate) top: *mut u8,
    pub(crate) pending_extend: u8,
}

pub(crate) struct Cursor {
    prev: NonNull<Hole>,
    hole: NonNull<Hole>,
    top: *mut u8,
}

// A block containing free memory (linked-list)
pub(crate) struct Hole {
    pub size: usize,
    pub next: Option<NonNull<Hole>>,
}

// Basic information about a hole
#[derive(Debug, Clone, Copy)]
struct HoleInfo {
    addr: *mut u8,
    size: usize,
}

impl Cursor {
    fn next(mut self) -> Option<Self> {
        unsafe {
            self.hole.as_mut().next.map(|nhole| Cursor {
                prev: self.hole,
                hole: nhole,
                top: self.top,
            })
        }
    }

    fn current(&self) -> &Hole {
        unsafe { self.hole.as_ref() }
    }

    fn previous(&self) -> &Hole {
        unsafe { self.prev.as_ref() }
    }

    fn split_current(self, required_layout: Layout) -> Result<(*mut u8, usize), Self> {
        let front_padding;
        let alloc_ptr;
        let alloc_size;
        let back_padding;

        {
            let hole_size = self.current().size;
            let hole_addr_u8 = self.hole.as_ptr().cast::<u8>();
            let required_size = required_layout.size();
            let required_align = required_layout.align();

            if hole_size < required_size {
                return Err(self);
            }

            // Try to break the node into: ([front_padding], allocation, [back_padding])
            let aligned_addr = if hole_addr_u8 == align_up(hole_addr_u8, required_align) {
                front_padding = None;
                hole_addr_u8
            } else {
                let new_start = hole_addr_u8.wrapping_add(HoleList::min_size());

                let aligned_addr = align_up(new_start, required_align);
                front_padding = Some(HoleInfo {
                    addr: hole_addr_u8,
                    size: (aligned_addr as usize) - (hole_addr_u8 as usize),
                });
                aligned_addr
            };

            let allocation_end = aligned_addr.wrapping_add(required_size);
            let hole_end = hole_addr_u8.wrapping_add(hole_size);

            if allocation_end > hole_end {
                // hole is too small
                return Err(self);
            }

            alloc_ptr = aligned_addr;
            alloc_size = required_size;

            let back_padding_size = hole_end as usize - allocation_end as usize;
            back_padding = if back_padding_size == 0 {
                None
            } else {
                let hole_layout = Layout::new::<Hole>();
                let back_padding_start = align_up(allocation_end, hole_layout.align());
                let back_padding_end = back_padding_start.wrapping_add(hole_layout.size());

                if back_padding_end <= hole_end {
                    Some(HoleInfo {
                        addr: back_padding_start,
                        size: back_padding_size,
                    })
                } else {
                    return Err(self);
                }
            };
        }


        let Cursor {
            mut prev, mut hole, ..
        } = self;

        // Remove the current location from the previous node
        unsafe {
            prev.as_mut().next = None;
        }

        // Take the next node out of our current node
        let maybe_next_addr: Option<NonNull<Hole>> = unsafe { hole.as_mut().next.take() };

        match (front_padding, back_padding) {
            (None, None) => {
                unsafe {
                    prev.as_mut().next = maybe_next_addr;
                }
            }
            (None, Some(singlepad)) | (Some(singlepad), None) => unsafe {
                let singlepad_ptr = singlepad.addr.cast::<Hole>();
                singlepad_ptr.write(Hole {
                    size: singlepad.size,
                    next: maybe_next_addr,
                });

                prev.as_mut().next = Some(NonNull::new_unchecked(singlepad_ptr));
            },
            (Some(frontpad), Some(backpad)) => unsafe {
                let backpad_ptr = backpad.addr.cast::<Hole>();
                backpad_ptr.write(Hole {
                    size: backpad.size,
                    next: maybe_next_addr,
                });

                let frontpad_ptr = frontpad.addr.cast::<Hole>();
                frontpad_ptr.write(Hole {
                    size: frontpad.size,
                    next: Some(NonNull::new_unchecked(backpad_ptr)),
                });

                prev.as_mut().next = Some(NonNull::new_unchecked(frontpad_ptr));
            },
        }

        Ok((alloc_ptr, alloc_size))
    }
}

fn check_merge_top(mut node: NonNull<Hole>, top: *mut u8) {
    let node_u8 = node.as_ptr().cast::<u8>();
    let node_sz = unsafe { node.as_ref().size };

    // If this is the last node, see if we can to merge to the end
    let end = node_u8.wrapping_add(node_sz);
    let hole_layout = Layout::new::<Hole>();
    if end < top {
        let next_hole_end = align_up(end, hole_layout.align()).wrapping_add(hole_layout.size());

        if next_hole_end > top {
            let offset = (top as usize) - (end as usize);
            unsafe {
                node.as_mut().size += offset;
            }
        }
    }
}

fn check_merge_bottom(node: NonNull<Hole>, bottom: *mut u8) -> NonNull<Hole> {
    // If this is the first node, see if we can merge it to the beginning
    if bottom.wrapping_add(core::mem::size_of::<Hole>()) > node.as_ptr().cast::<u8>() {
        let offset = (node.as_ptr() as usize) - (bottom as usize);
        let size = unsafe { node.as_ref() }.size + offset;
        unsafe { make_hole(bottom, size) }
    } else {
        node
    }
}

impl HoleList {
    pub const fn empty() -> HoleList {
        HoleList {
            first: Hole {
                size: 0,
                next: None,
            },
            bottom: null_mut(),
            top: null_mut(),
            pending_extend: 0,
        }
    }

    pub(crate) fn cursor(&mut self) -> Option<Cursor> {
        if let Some(hole) = self.first.next {
            Some(Cursor {
                hole,
                prev: NonNull::new(&mut self.first)?,
                top: self.top,
            })
        } else {
            None
        }
    }

    pub unsafe fn new(hole_addr: *mut u8, hole_size: usize) -> HoleList {
        assert_eq!(size_of::<Hole>(), Self::min_size());
        assert!(hole_size >= size_of::<Hole>());

        let aligned_hole_addr = align_up(hole_addr, align_of::<Hole>());
        let requested_hole_size = hole_size - ((aligned_hole_addr as usize) - (hole_addr as usize));
        let aligned_hole_size = align_down_size(requested_hole_size, align_of::<Hole>());
        assert!(aligned_hole_size >= size_of::<Hole>());

        let ptr = aligned_hole_addr as *mut Hole;
        ptr.write(Hole {
            size: aligned_hole_size,
            next: None,
        });

        assert_eq!(
            hole_addr.wrapping_add(hole_size),
            aligned_hole_addr.wrapping_add(requested_hole_size)
        );

        HoleList {
            first: Hole {
                size: 0,
                next: Some(NonNull::new_unchecked(ptr)),
            },
            bottom: aligned_hole_addr,
            top: aligned_hole_addr.wrapping_add(aligned_hole_size),
            pending_extend: (requested_hole_size - aligned_hole_size) as u8,
        }
    }

    /// Aligns the given layout for use with `HoleList`.
    pub fn align_layout(layout: Layout) -> Layout {
        let mut size = layout.size();
        if size < Self::min_size() {
            size = Self::min_size();
        }
        let size = align_up_size(size, mem::align_of::<Hole>());
        Layout::from_size_align(size, layout.align()).unwrap()
    }

    /// Searches the list for a big enough hole.
    #[allow(clippy::result_unit_err)]
    pub fn allocate_first_fit(&mut self, layout: Layout) -> Result<(NonNull<u8>, Layout), ()> {
        let aligned_layout = Self::align_layout(layout);
        let mut cursor = self.cursor().ok_or(())?;

        loop {
            match cursor.split_current(aligned_layout) {
                Ok((ptr, _len)) => {
                    return Ok((NonNull::new(ptr).ok_or(())?, aligned_layout));
                }
                Err(curs) => {
                    cursor = curs.next().ok_or(())?;
                }
            }
        }
    }

    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) -> Layout {
        let aligned_layout = Self::align_layout(layout);
        deallocate(self, ptr.as_ptr(), aligned_layout.size());
        aligned_layout
    }

    pub fn min_size() -> usize {
        size_of::<usize>() * 2
    }

    #[cfg(test)]
    pub fn first_hole(&self) -> Option<(*const u8, usize)> {
        self.first.next.as_ref().map(|hole| {
            (hole.as_ptr() as *mut u8 as *const u8, unsafe {
                hole.as_ref().size
            })
        })
    }

    pub(crate) unsafe fn extend(&mut self, by: usize) {
        assert!(!self.top.is_null(), "tried to extend an empty heap");

        let top = self.top;

        let dead_space = top.align_offset(align_of::<Hole>());
        debug_assert_eq!(
            0, dead_space,
            "dead space detected during extend: {} bytes. This means top was unaligned",
            dead_space
        );

        debug_assert!(
            (self.pending_extend as usize) < Self::min_size(),
            "pending extend was larger than expected"
        );

        // join this extend request with any pending (but not yet acted on) extension
        let extend_by = self.pending_extend as usize + by;

        let minimum_extend = Self::min_size();
        if extend_by < minimum_extend {
            self.pending_extend = extend_by as u8;
            return;
        }

        // only extend up to another valid boundary
        let new_hole_size = align_down_size(extend_by, align_of::<Hole>());
        let layout = Layout::from_size_align(new_hole_size, 1).unwrap();

        // instantiate the hole by forcing a deallocation on the new memory
        self.deallocate(NonNull::new_unchecked(top as *mut u8), layout);
        self.top = top.add(new_hole_size);

        // save extra bytes given to extend that weren't aligned to the hole size
        self.pending_extend = (extend_by - new_hole_size) as u8;
    }
}

unsafe fn make_hole(addr: *mut u8, size: usize) -> NonNull<Hole> {
    let hole_addr = addr.cast::<Hole>();
    debug_assert_eq!(
        addr as usize % align_of::<Hole>(),
        0,
        "Hole address not aligned!",
    );
    hole_addr.write(Hole { size, next: None });
    NonNull::new_unchecked(hole_addr)
}

impl Cursor {
    fn try_insert_back(self, node: NonNull<Hole>, bottom: *mut u8) -> Result<Self, Self> {
        // Covers the case where the new hole exists BEFORE the current pointer,
        // which only happens when previous is the stub pointer
        if node < self.hole {
            let node_u8 = node.as_ptr().cast::<u8>();
            let node_size = unsafe { node.as_ref().size };
            let hole_u8 = self.hole.as_ptr().cast::<u8>();

            let Cursor {
                mut prev,
                hole,
                top,
            } = self;
            unsafe {
                let mut node = check_merge_bottom(node, bottom);
                prev.as_mut().next = Some(node);
                node.as_mut().next = Some(hole);
            }
            Ok(Cursor {
                prev,
                hole: node,
                top,
            })
        } else {
            Err(self)
        }
    }

    fn try_insert_after(&mut self, mut node: NonNull<Hole>) -> Result<(), ()> {
        let node_u8 = node.as_ptr().cast::<u8>();
        let node_size = unsafe { node.as_ref().size };

        // If we have a next, does the node overlap next?
        if let Some(next) = self.current().next.as_ref() {
            if node < *next {
                let node_u8 = node_u8 as *const u8;
            } else {
                // The new hole isn't between current and next.
                return Err(());
            }
        }

        // either no "next" pointer, or the hole is between current and "next"

        let hole_u8 = self.hole.as_ptr().cast::<u8>();
        let hole_size = self.current().size;

        // insert after.
        unsafe {
            let maybe_next = self.hole.as_mut().next.replace(node);
            node.as_mut().next = maybe_next;
        }

        Ok(())
    }

    // Merge the current node with up to n following nodes
    fn try_merge_next_n(self, max: usize) {
        let Cursor {
            prev: _,
            mut hole,
            top,
            ..
        } = self;

        for _ in 0..max {
            // Is there a next node?
            let mut next = if let Some(next) = unsafe { hole.as_mut() }.next.as_ref() {
                *next
            } else {
                // Since there is no NEXT node, we need to check whether the current
                // hole SHOULD extend to the end, but doesn't. This would happen when
                // there isn't enough remaining space to place a hole after the current
                // node's placement.
                check_merge_top(hole, top);
                return;
            };

            let hole_u8 = hole.as_ptr().cast::<u8>();
            let hole_sz = unsafe { hole.as_ref().size };
            let next_u8 = next.as_ptr().cast::<u8>();
            let end = hole_u8.wrapping_add(hole_sz);

            let touching = end == next_u8;

            if touching {
                let next_sz;
                let next_next;
                unsafe {
                    let next_mut = next.as_mut();
                    next_sz = next_mut.size;
                    next_next = next_mut.next.take();
                }
                unsafe {
                    let hole_mut = hole.as_mut();
                    hole_mut.next = next_next;
                    hole_mut.size += next_sz;
                }
                // Just merged the next item. DON'T move the cursor, as we can
                // just try to merge the next_next, which is now our next.
            } else {
                // Welp, not touching, can't merge. Move to the next node.
                hole = next;
            }
        }
    }
}

fn deallocate(list: &mut HoleList, addr: *mut u8, size: usize) {
    let hole = unsafe { make_hole(addr, size) };

    let cursor = if let Some(cursor) = list.cursor() {
        cursor
    } else {
        let hole = check_merge_bottom(hole, list.bottom);
        check_merge_top(hole, list.top);
        list.first.next = Some(hole);
        return;
    };

    let (cursor, n) = match cursor.try_insert_back(hole, list.bottom) {
        Ok(cursor) => {
            (cursor, 1)
        }
        Err(mut cursor) => {
            while let Err(()) = cursor.try_insert_after(hole) {
                cursor = cursor
                    .next()
                    .expect("Reached end of holes without finding deallocation hole!");
            }
            (cursor, 2)
        }
    };

    cursor.try_merge_next_n(n);
}
