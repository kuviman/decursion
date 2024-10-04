use future::LocalBoxFuture;
use futures::prelude::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

pub struct Decurser {
    call_stack: Vec<Rc<RefCell<LocalBoxFuture<'static, ()>>>>,
}

pub fn run_decursing<T: 'static>(f: impl Future<Output = T> + 'static) -> T {
    let (mut sender, receiver) = async_oneshot::oneshot();
    let decurser = RefCell::new(Decurser {
        call_stack: vec![Rc::new(RefCell::new(
            f.map(move |result| {
                sender.send(result).expect(
                    "the root decursing future is completed from outside of run_decursing???",
                )
            })
            .boxed_local(),
        ))],
    });
    DECURSER.set(&decurser, || {
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            let decurser_ref = decurser.borrow();
            let Some(f) = decurser_ref.call_stack.last() else {
                break;
            };
            let f = Rc::clone(f);
            std::mem::drop(decurser_ref);

            match f.borrow_mut().poll_unpin(&mut cx) {
                std::task::Poll::Ready(()) => {
                    // the innermost function in the call stack has exited
                    decurser.borrow_mut().call_stack.pop();
                }
                std::task::Poll::Pending => {
                    // this should mean that we have called inner function with decurse
                }
            };
        }
        receiver
            .try_recv()
            .map_err(|_| ())
            .expect("run_decursing is completed but the future was dropped???")
    })
}

scoped_thread_local!(static DECURSER: RefCell<Decurser>);

pub struct RecursedFuture<T>(async_oneshot::Receiver<T>);

impl<T> Future for RecursedFuture<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.get_mut()
            .0
            .poll_unpin(cx)
            .map(|result| result.expect("the sender was dropped???"))
    }
}

pub trait FutureExt: Future {
    fn decurse(self) -> RecursedFuture<Self::Output>;
}

impl<F: Future> FutureExt for F
where
    Self: 'static,
{
    fn decurse(self) -> RecursedFuture<F::Output> {
        let (mut sender, receiver) = async_oneshot::oneshot();
        if !DECURSER.is_set() {
            panic!("You can only decurse when inside a decursing context");
        }
        DECURSER.with(|decurser| {
            let mut decurser = decurser.borrow_mut();
            decurser
                .call_stack
                .push(Rc::new(RefCell::new(Box::pin(self.map(move |result| {
                    sender
                        .send(result)
                        .expect("the caller of decursed was dropped???")
                })))));
        });
        RecursedFuture(receiver)
    }
}
