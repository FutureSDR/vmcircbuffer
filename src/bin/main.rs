use std::iter::repeat_with;
use std::sync::{Arc, Barrier};
use std::thread;
use std::thread::JoinHandle;
use std::time;

use double_mapped_circular_buffer::sync::Circular;
use double_mapped_circular_buffer::sync::Reader;

struct VectorSource;
impl VectorSource {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<A>(input: Vec<A>) -> Source<A>
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
struct Source<A: Send + Sync + 'static> {
    f: Option<Box<dyn FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static>>,
}

impl<A: Send + Sync> Source<A> {
    pub fn new(f: impl FnMut(&mut [A]) -> Option<usize> + Send + Sync + 'static) -> Source<A> {
        Source {
            f: Some(Box::new(f)),
        }
    }

    pub fn run(&mut self, barrier: Arc<Barrier>) -> (Reader<A>, JoinHandle<()>) {
        let w = Circular::new::<A>().unwrap();
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

            println!("Source terminated");
        });

        (r, handle)
    }
}

struct CopyBlock;
impl CopyBlock {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<A>() -> Middle<A, A>
    where
        A: Send + Sync + Clone + 'static,
    {
        Middle::new(|input: &[A], output: &mut [A]| output.clone_from_slice(input))
    }
}

#[allow(clippy::type_complexity)]
struct Middle<A, B>
where
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    f: Option<Box<dyn FnMut(&[A], &mut [B]) + Send + Sync + 'static>>,
}

impl<A, B> Middle<A, B>
where
    A: Send + Sync + 'static,
    B: Send + Sync + 'static,
{
    pub fn new(f: impl FnMut(&[A], &mut [B]) + Send + Sync + 'static) -> Middle<A, B> {
        Middle {
            f: Some(Box::new(f)),
        }
    }

    pub fn run(&mut self, reader: Reader<A>, barrier: Arc<Barrier>) -> (Reader<B>, JoinHandle<()>) {
        let w = Circular::new::<B>().unwrap();
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

    pub fn run(&mut self, r: Reader<A>, barrier: Arc<Barrier>) -> JoinHandle<Vec<A>> {
        let mut items = self.items.take().unwrap();

        thread::spawn(move || {
            barrier.wait();

            while let Some(s) = r.slice() {
                items.extend_from_slice(s);
                r.consume(s.len());
            }

            println!("Sink terminated");
            items
        })
    }
}

fn main() {
    let n_samples = 3231233;
    let input: Vec<f32> = repeat_with(rand::random::<f32>).take(n_samples).collect();

    let n_copy = 1230;
    let barrier = Arc::new(Barrier::new(n_copy + 3));

    let mut src = VectorSource::new(input);
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
    println!("rxed vec of len {}", output.len());
    println!("processing took: {}", elapsed.as_secs_f64());
}
