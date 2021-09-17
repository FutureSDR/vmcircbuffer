use std::iter::repeat_with;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time;
use std::thread::JoinHandle;

use double_mapped_circular_buffer::sync::Circular;
use double_mapped_circular_buffer::sync::Reader;

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

    pub fn run(
        &mut self,
        barrier: Arc<Barrier>,
    ) -> (Reader<A>, JoinHandle<()>) {
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

            println!("source terminated");
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

    pub fn run(
        &mut self,
        r: Reader<A>,
        barrier: Arc<Barrier>,
    ) -> JoinHandle<Vec<A>> {
        let mut items = self.items.take().unwrap();

        thread::spawn(move || {
            barrier.wait();

            while let Some(s) = r.slice() {
                items.extend_from_slice(s);
                r.consume(s.len());
            }

            println!("sink terminated");
            items
        })
    }
}

// fn main() {
//     let n_samples = 3231233;
//     let input: Vec<f32> = repeat_with(rand::random::<f32>).take(n_samples).collect();

//     let mut i = 0;
//     let mut src = Source::new(move |s: &mut [f32]| -> Option<usize> {
//         if i < n_samples {
//             let len = std::cmp::min(s.len(), n_samples - i);
//             s[0..len].clone_from_slice(&input[i..i + len]);
//             i += len;
//             Some(len)
//         } else {
//             None
//         }
//     });
//     let mut snk = Sink::new(n_samples);

//     let barrier = Arc::new(Barrier::new(3));
//     let (reader, _) = src.run(Arc::clone(&barrier));
//     let handle = snk.run(reader, Arc::clone(&barrier));

//     let now = time::Instant::now();
//     barrier.wait();
//     let output = handle.join().unwrap();
//     let elapsed = now.elapsed();
//     println!("rxed vec of len {}", output.len());
//     println!("processing took: {}", elapsed.as_secs_f64());
// }

fn main() {
    let w = Circular::new::<f32>().unwrap();
    let r = w.add_reader();

    w.produce(w.slice().len());


    thread::spawn(move || {
        println!("trying to get write buffer ");
        let _ = w.slice();
        println!("got write buffer ");
    });
    
    std::thread::sleep(std::time::Duration::from_millis(1000));
    println!("reading");
    r.consume(r.slice().unwrap().len());
}
