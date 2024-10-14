use future::LocalBoxFuture;
use futures::prelude::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::RefCell;
use std::future::Future;
use std::marker::PhantomData;
use std::rc::Rc;

pub struct Decurser {
    /// not actually static
    call_stack: Vec<Rc<RefCell<LocalBoxFuture<'static, ()>>>>,
}

fn save_to_call_stack<T>(
    mut sender: async_oneshot::Sender<T>,
    f: impl Future<Output = T>,
) -> Rc<RefCell<LocalBoxFuture<'static, ()>>> {
    Rc::new(RefCell::new({
        let future = f
            .map(move |result| {
                sender.send(result).expect(
                    "the root decursing future is completed from outside of run_decursing???",
                )
            })
            .boxed_local();
        unsafe {
            std::mem::transmute::<LocalBoxFuture<'_, ()>, LocalBoxFuture<'static, ()>>(future)
        }
    }))
}

pub fn run_decursing<'a, T: 'a>(f: impl Future<Output = T> + 'a) -> T {
    let (sender, receiver) = async_oneshot::oneshot();
    let decurser = RefCell::new(Decurser {
        call_stack: vec![save_to_call_stack(sender, f)],
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

pub struct DecursedFuture<F: Future> {
    receiver: async_oneshot::Receiver<F::Output>,
    phantom_data: PhantomData<*const F>,
}

impl<F: Future> Future for DecursedFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.get_mut()
            .receiver
            .poll_unpin(cx)
            .map(|result| result.expect("the sender was dropped???"))
    }
}

pub trait FutureExt: Future {
    type Decursed;
    fn decurse(self) -> Self::Decursed;
}

// my Linux crashes when I launch Chrome
impl<F: Future> FutureExt for F {
    type Decursed = DecursedFuture<F>;
    fn decurse(self) -> DecursedFuture<F> {
        let (sender, receiver) = async_oneshot::oneshot();
        if !DECURSER.is_set() {
            panic!(
                "You can only decurse when inside a decursing context: run with `run_decursing`"
            );
        }
        DECURSER.with(|decurser| {
            let mut decurser = decurser.borrow_mut();
            decurser.call_stack.push(save_to_call_stack(sender, self));
        });
        DecursedFuture {
            receiver,
            phantom_data: PhantomData,
        }
    }
}
