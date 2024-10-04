use future::LocalBoxFuture;
use futures::prelude::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::UnsafeCell;
use std::future::Future;

pub struct Decurser {
    call_stack: Vec<UnsafeCell<LocalBoxFuture<'static, ()>>>,
    returned_value: Vec<u8>,
}

impl Decurser {
    unsafe fn push(this: &UnsafeCell<Self>, future: impl Future) {
        let this = &mut *this.get();
        this.call_stack.push(UnsafeCell::new(
            future
                .map(move |result| {
                    let memory_needed = std::mem::size_of_val(&result);
                    let current_memory = this.returned_value.capacity();
                    if memory_needed > current_memory {
                        this.returned_value.reserve(memory_needed - current_memory);
                    }
                    std::ptr::write_unaligned(this.returned_value.as_mut_ptr() as *mut _, result);
                })
                .boxed_local(),
        ))
    }
    unsafe fn get_returned_value<T>(this: &UnsafeCell<Self>) -> T {
        let this = &mut *this.get();
        std::ptr::read_unaligned(this.returned_value.as_ptr() as *const T)
    }
}

pub fn run_decursing<T: 'static>(f: impl Future<Output = T> + 'static) -> T {
    let decurser = UnsafeCell::new(Decurser {
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
            let Some(f) = unsafe { &mut *decurser.get() }.call_stack.last() else {
                break;
            };
            match unsafe { &mut *f.get() }.poll_unpin(&mut cx) {
                std::task::Poll::Ready(()) => {
                    // the innermost function in the call stack has exited
                    unsafe { &mut *decurser.get() }.call_stack.pop();
                }
                std::task::Poll::Pending => {
                    // this should mean that we have called inner function with decurse
                }
            };
        }
        unsafe { &mut *decurser.get() }.get_returned_value()
    })
}

scoped_thread_local!(static DECURSER: UnsafeCell<Decurser>);

pub struct RecursedFuture<T>(async_oneshot::Receiver<T>);

impl<T> Future for RecursedFuture<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            self.map_unchecked_mut(|this| &mut this.0)
                .poll(cx)
                .map(|result| result.expect("the sender was dropped???"))
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
            let decurser = unsafe { &mut *decurser.get() };
            decurser.call_stack.push(UnsafeCell::new(Box::pin(
                self.map(move |result| sender.send(result).unwrap()),
            )));
        });
        RecursedFuture(receiver)
    }
}
