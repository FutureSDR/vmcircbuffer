use std::iter::repeat_with;
use std::marker::PhantomData;
use std::sync::{Arc, Barrier};
use std::thread;
use std::thread::JoinHandle;
use std::time;

use vmcircbuffer::sync::Circular;
use vmcircbuffer::sync::Reader;

const MIN_ITEMS: usize = 16384;

struct VectorSource;
impl VectorSource {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<A>(
        input: Vec<A>,
    ) -> Source<impl FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static, A>
    where
        A: Send + Sync + Clone + 'static,
    {
        let mut i = 0;
        let n_samples = input.len();
        Source::new(move |s: &mut [A]| -> Option<usize> {
            if i < n_samples {
                let len = std::cmp::min(s.len(), n_samples - i);
                s[0..len].clone_from_slice(&input[i..i + len]);
                i += len;
                Some(len)
            } else {
                None
            }
        })
    }
}

#[allow(clippy::type_complexity)]
struct Source<F: FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static, A: Send + Sync + 'static>
{
    f: Option<F>,
    _p: PhantomData<A>,
}

impl<F: FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static, A: Send + Sync> Source<F, A> {
    pub fn new(f: F) -> Source<F, A> {
        Source {
            f: Some(f),
            _p: PhantomData,
        }
    }

    pub fn run(&mut self, barrier: Arc<Barrier>) -> (Reader<A>, JoinHandle<()>) {
        let mut w = Circular::with_capacity::<A>(MIN_ITEMS).unwrap();
        let r = w.add_reader();
        let mut f = self.f.take().unwrap();

        let handle = thread::spawn(move || {
            barrier.wait();

            loop {
                let s = w.slice();
                if let Some(n) = f(s) {
                    w.produce(n);
                } else {
                    break;
                }
            }
        });

        (r, handle)
    }
}

struct CopyBlock;
impl CopyBlock {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<A>() -> Middle<impl FnMut(&[A], &mut [A]) + Send + Sync + 'static, A, A>
    where
        A: Send + Sync + Clone + 'static,
    {
        Middle::new(|input: &[A], output: &mut [A]| output.clone_from_slice(input))
    }
}

#[allow(clippy::type_complexity)]
struct Middle<F, A, B>
where
    F: FnMut(&[A], &mut [B]) + Send + Sync + 'static,
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    f: Option<F>,
    _p1: PhantomData<A>,
    _p2: PhantomData<B>,
}

impl<F, A, B> Middle<F, A, B>
where
    F: FnMut(&[A], &mut [B]) + Send + Sync + 'static,
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    pub fn new(f: F) -> Middle<F, A, B> {
        Middle {
            f: Some(f),
            _p1: PhantomData,
            _p2: PhantomData,
        }
    }

    pub fn run(
        &mut self,
        mut reader: Reader<A>,
        barrier: Arc<Barrier>,
    ) -> (Reader<B>, JoinHandle<()>) {
        let mut w = Circular::with_capacity::<B>(MIN_ITEMS).unwrap();
        let r = w.add_reader();
        let mut f = self.f.take().unwrap();

        let handle = thread::spawn(move || {
            barrier.wait();

            while let Some(input) = reader.slice() {
                let output = w.slice();
                let n = std::cmp::min(input.len(), output.len());
                f(&input[0..n], &mut output[0..n]);
                reader.consume(n);
                w.produce(n);
            }
        });

        (r, handle)
    }
}

struct Sink<A: Clone + Send + Sync + 'static> {
    items: Option<Vec<A>>,
}

impl<A: Clone + Send + Sync + 'static> Sink<A> {
    pub fn new(capacity: usize) -> Sink<A> {
        Sink {
            items: Some(Vec::with_capacity(capacity)),
        }
    }

    pub fn run(&mut self, mut r: Reader<A>, barrier: Arc<Barrier>) -> JoinHandle<Vec<A>> {
        let mut items = self.items.take().unwrap();

        thread::spawn(move || {
            barrier.wait();

            while let Some(s) = r.slice() {
                items.extend_from_slice(s);
                let l = s.len();
                r.consume(l);
            }

            items
        })
    }
}

fn main() {
    let n_samples = 20_000_000;
    let input: Vec<f32> = repeat_with(rand::random::<f32>).take(n_samples).collect();

    let n_copy = 200;
    let barrier = Arc::new(Barrier::new(n_copy + 3));

    let mut src = VectorSource::new(input.clone());
    let (mut reader, _) = src.run(Arc::clone(&barrier));

    for _ in 0..n_copy {
        let mut cpy = CopyBlock::new::<f32>();
        let (a, _) = cpy.run(reader, Arc::clone(&barrier));
        reader = a;
    }

    let mut snk = Sink::new(n_samples);
    let handle = snk.run(reader, Arc::clone(&barrier));

    let now = time::Instant::now();
    barrier.wait();
    let output = handle.join().unwrap();
    let elapsed = now.elapsed();
    assert_eq!(input, output);
    println!("data matches");
    println!("runtime (in s): {}", elapsed.as_secs_f64());
}
