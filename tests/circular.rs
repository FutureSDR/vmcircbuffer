use rand::distributions::{Distribution, Uniform};
use std::iter::repeat_with;

use vmcircbuffer::asynchronous;
use vmcircbuffer::nonblocking;
use vmcircbuffer::sync::Circular;

#[test]
fn create_many() {
    let mut v = Vec::new();
    for _ in 0..100 {
        v.push(Circular::new::<u8>().unwrap());
    }
}

#[test]
fn zero_size() {
    let w = Circular::new::<u8>().unwrap();
    assert!(!w.slice().is_empty());
}

#[test]
fn no_reader() {
    let w = Circular::new::<u8>().unwrap();
    let s = w.slice();
    w.produce(s.len());
    assert!(!w.slice().is_empty());
}

#[test]
fn late_reader() {
    let w = Circular::new::<u32>().unwrap();
    let s = w.slice();
    for (i, v) in s.iter_mut().take(200).enumerate() {
        *v = i as u32;
    }
    w.produce(100);

    let r = w.add_reader();
    assert_eq!(r.try_slice().unwrap().len(), 0);
    w.produce(100);
    assert_eq!(r.slice().unwrap().len(), 100);
    for (i, v) in r.slice().unwrap().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn several_readers() {
    let w = Circular::new::<u32>().unwrap();

    let r1 = w.add_reader();
    let r2 = w.add_reader();

    for (i, v) in w.slice().iter_mut().enumerate() {
        *v = i as u32;
    }
    let all = w.slice().len();
    assert_eq!(r1.try_slice().unwrap().len(), 0);
    w.produce(w.slice().len());
    assert_eq!(r2.slice().unwrap().len(), all);

    r1.consume(100);

    assert_eq!(r1.slice().unwrap().len(), all - 100);
    for (i, v) in r1.slice().unwrap().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn fuzz_sync() {
    let w = Circular::new::<u32>().unwrap();
    let r = w.add_reader();
    let size = w.slice().len();

    let input: Vec<u32> = repeat_with(rand::random::<u32>).take(1231233).collect();

    let mut rng = rand::thread_rng();
    let n_writes_dist = Uniform::from(0..4);
    let n_samples_dist = Uniform::from(0..size / 2);

    let mut w_off = 0;
    let mut r_off = 0;

    while r_off < input.len() {
        let n_writes = n_writes_dist.sample(&mut rng);
        for _ in 0..n_writes {
            let s = w.slice();
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
}

#[test]
fn fuzz_nonblocking() {
    let w = nonblocking::Circular::new::<u32>().unwrap();
    let r = w.add_reader();
    let size = w.try_slice().len();

    let input: Vec<u32> = repeat_with(rand::random::<u32>).take(1231233).collect();

    let mut rng = rand::thread_rng();
    let n_writes_dist = Uniform::from(0..4);
    let n_samples_dist = Uniform::from(0..size / 2);

    let mut w_off = 0;
    let mut r_off = 0;

    while r_off < input.len() {
        let n_writes = n_writes_dist.sample(&mut rng);
        for _ in 0..n_writes {
            let s = w.try_slice();
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

#[test]
fn block() {
    let w = Circular::new::<f32>().unwrap();
    let r = w.add_reader();

    w.produce(w.slice().len());

    let now = std::time::Instant::now();
    let delay = std::time::Duration::from_millis(1000);

    std::thread::spawn(move || {
        std::thread::sleep(delay);
        r.consume(r.slice().unwrap().len());
    });

    let _ = w.slice();
    assert!(now.elapsed() > delay);
}

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
fn minimal() {
    let w = Circular::new::<u32>().unwrap();
    let r = w.add_reader();

    for v in w.slice() {
        *v = 123;
    }
    w.produce(w.slice().len());

    for v in r.slice().unwrap() {
        assert_eq!(*v, 123);
    }
}
