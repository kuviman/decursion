use future::LocalBoxFuture;
use futures::prelude::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::UnsafeCell;
use std::future::Future;

pub struct Decurser {
    call_stack: Vec<UnsafeCell<LocalBoxFuture<'static, ()>>>,
}

pub fn run_decursing<T: 'static>(f: impl Future<Output = T> + 'static) -> T {
    let (mut sender, receiver) = async_oneshot::oneshot();
    let decurser = UnsafeCell::new(Decurser {
        call_stack: vec![UnsafeCell::new(
            f.map(move |result| sender.send(result).unwrap())
                .boxed_local(),
        )],
    });
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
        receiver.try_recv().map_err(|_| ()).unwrap()
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
    fn decurse(self) -> LocalBoxFuture<'static, Self::Output>;
}

impl<F: Future> FutureExt for F
where
    Self: 'static,
{
    fn decurse(self) -> LocalBoxFuture<'static, Self::Output> {
        let (mut sender, receiver) = async_oneshot::oneshot();
        if !DECURSER.is_set() {
            panic!("You can only decurse when inside a decursing context");
        }
        DECURSER.with(|decurser| {
            let decurser = unsafe { &mut *decurser.get() };
            decurser.call_stack.push(UnsafeCell::new(Box::pin(
                self.map(move |result| sender.send(result).unwrap()),
            )));
        });
        receiver.map(|result| result.unwrap()).boxed_local()
    }
}
