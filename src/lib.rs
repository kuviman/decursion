use futures::prelude::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomData;
use std::task::Poll;

pub struct Decurser {
    // TODO drops?
    allocator: bumpalo::Bump,
    // this is not actually static KEKW
    call_stack: Vec<*mut dyn Future<Output = ()>>,
    returned_value: Vec<u8>,
}

impl Decurser {
    unsafe fn push(this: &UnsafeCell<Self>, future: impl Future + 'static) {
        let this_ptr = this.get();
        let this = &mut *this_ptr;
        this.call_stack.push({
            let future = future.map(move |result| {
                let this = &mut *this_ptr;
                let memory_needed = std::mem::size_of_val(&result);
                let current_memory = this.returned_value.capacity();
                if memory_needed > current_memory {
                    this.returned_value.reserve(memory_needed - current_memory);
                }
                std::ptr::write_unaligned(this.returned_value.as_mut_ptr() as *mut _, result);
            });
            this.allocator.alloc(future) as *mut _ as *mut dyn Future<Output = ()>
        })
    }
    unsafe fn get_returned_value<T>(this: &UnsafeCell<Self>) -> T {
        let this = &mut *this.get();
        std::ptr::read_unaligned(this.returned_value.as_ptr() as *const T)
    }
}

pub fn run_decursing<T: 'static>(f: impl Future<Output = T> + 'static) -> T {
    let decurser = UnsafeCell::new(Decurser {
        allocator: bumpalo::Bump::new(),
        call_stack: Vec::new(),
        returned_value: Vec::new(),
    });
    unsafe {
        Decurser::push(&decurser, f);
    }
    DECURSER.set(&decurser, || {
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            let Some(f) = unsafe { &mut *decurser.get() }.call_stack.last().copied() else {
                break;
            };
            let f = std::pin::pin!(f);
            let f = unsafe { f.map_unchecked_mut(|f| &mut **f) };
            match f.poll(&mut cx) {
                std::task::Poll::Ready(()) => {
                    // the innermost function in the call stack has exited
                    unsafe { &mut *decurser.get() }.call_stack.pop();
                }
                std::task::Poll::Pending => {
                    // this should mean that we have called inner function with decurse
                }
            };
        }
        unsafe { Decurser::get_returned_value(&decurser) }
    })
}

scoped_thread_local!(static DECURSER: UnsafeCell<Decurser>);

pub struct RecursedFuture<T> {
    polled: bool,
    phantom_data: PhantomData<T>,
}

impl<T> Future for RecursedFuture<T> {
    type Output = T;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.polled {
            false => {
                unsafe {
                    self.get_unchecked_mut().polled = true;
                }
                Poll::Pending
            }
            true => Poll::Ready(
                DECURSER.with(|decurser| unsafe { Decurser::get_returned_value(decurser) }),
            ),
        }
    }
}

pub trait FutureExt: Future {
    fn decurse(self) -> RecursedFuture<<Self as Future>::Output>;
}

impl<F: Future> FutureExt for F
where
    Self: 'static,
{
    fn decurse(self) -> RecursedFuture<<F as Future>::Output> {
        if !DECURSER.is_set() {
            panic!("You can only decurse when inside a decursing context");
        }
        DECURSER.with(|decurser| {
            unsafe { Decurser::push(decurser, self) };
        });
        RecursedFuture {
            polled: false,
            phantom_data: PhantomData,
        }
    }
}
