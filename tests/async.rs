use rand::distributions::{Distribution, Uniform};
use std::iter::repeat_with;

use vmcircbuffer::asynchronous;

#[test]
fn wait() {
    smol::block_on(async {
        let mut w = asynchronous::Circular::new::<f32>().unwrap();
        let mut r = w.add_reader();

        let l = w.slice().await.len();
        w.produce(l);

        let now = std::time::Instant::now();
        let delay = std::time::Duration::from_millis(1000);

        smol::spawn(async move {
            smol::Timer::after(delay).await;
            let l = r.slice().await.unwrap().len();
            r.consume(l);
        })
        .detach();

        let _ = w.slice().await;
        assert!(now.elapsed() > delay);
    });
}

#[test]
fn fuzz_async() {
    smol::block_on(async {
        let mut w = asynchronous::Circular::new::<u32>().unwrap();
        let r = w.add_reader();
        let size = w.slice().await.len();

        let input: Vec<u32> = repeat_with(rand::random::<u32>).take(1231233).collect();

        let mut rng = rand::thread_rng();
        let n_writes_dist = Uniform::from(0..4);
        let n_samples_dist = Uniform::from(0..size / 2);

        let mut w_off = 0;
        let mut r_off = 0;

        while r_off < input.len() {
            let n_writes = n_writes_dist.sample(&mut rng);
            for _ in 0..n_writes {
                let s = w.slice().await;
                let n = std::cmp::min(s.len(), input.len() - w_off);
                let n = std::cmp::min(n, n_samples_dist.sample(&mut rng));

                for (i, v) in s.iter_mut().take(n).enumerate() {
                    *v = input[w_off + i];
                }
                w.produce(n);
                w_off += n;
            }

            let s = r.try_slice().unwrap();
            assert_eq!(s.len(), w_off - r_off);

            for (i, v) in s.iter().enumerate() {
                assert_eq!(*v, input[r_off + i]);
            }
            r.consume(s.len());
            r_off += s.len();
        }
    });
}
