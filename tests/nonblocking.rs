use rand::distributions::{Distribution, Uniform};
use std::iter::repeat_with;

use vmcircbuffer::nonblocking::Circular;

#[test]
fn create_many() {
    let mut v = Vec::new();
    for _ in 0..100 {
        v.push(Circular::new::<u8>().unwrap());
    }
}

#[test]
fn zero_size() {
    let mut w = Circular::new::<u8>().unwrap();
    assert!(!w.try_slice().is_empty());
}

#[test]
fn no_reader() {
    let mut w = Circular::new::<u8>().unwrap();
    let s = w.try_slice();
    let l = s.len();
    w.produce(l);
    assert!(!w.try_slice().is_empty());
}

#[test]
#[should_panic]
fn produce_too_much() {
    let mut w = Circular::new::<u8>().unwrap();
    let s = w.try_slice();
    let l = s.len();
    w.produce(l + 1);
}

#[test]
#[should_panic]
fn consume_too_much() {
    let mut w = Circular::new::<u8>().unwrap();
    let mut r = w.add_reader();
    let s = w.try_slice();
    let l = s.len();
    w.produce(l + 1);
    let s = r.try_slice().unwrap();
    let l = s.len();
    r.consume(l + 1);
}

#[test]
fn late_reader() {
    let mut w = Circular::new::<u32>().unwrap();
    let s = w.try_slice();
    for (i, v) in s.iter_mut().take(200).enumerate() {
        *v = i as u32;
    }
    w.produce(100);

    let mut r = w.add_reader();
    assert_eq!(r.try_slice().unwrap().len(), 0);
    w.produce(100);
    assert_eq!(r.try_slice().unwrap().len(), 100);
    for (i, v) in r.try_slice().unwrap().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn several_readers() {
    let mut w = Circular::new::<u32>().unwrap();

    let mut r1 = w.add_reader();
    let mut r2 = w.add_reader();

    for (i, v) in w.try_slice().iter_mut().enumerate() {
        *v = i as u32;
    }
    let all = w.try_slice().len();
    assert_eq!(r1.try_slice().unwrap().len(), 0);
    let l = w.try_slice().len();
    w.produce(l);
    assert_eq!(r2.try_slice().unwrap().len(), all);

    let _ = r1.try_slice();
    r1.consume(100);

    assert_eq!(r1.try_slice().unwrap().len(), all - 100);
    for (i, v) in r1.try_slice().unwrap().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn fuzz_nonblocking() {
    let mut w = Circular::new::<u32>().unwrap();
    let mut r = w.add_reader();
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
        let l = s.len();
        r.consume(l);
        r_off += l;
    }
}

#[test]
fn minimal() {
    let mut w = Circular::new::<u32>().unwrap();
    let mut r = w.add_reader();

    for v in w.try_slice() {
        *v = 123;
    }
    let l = w.try_slice().len();
    w.produce(l);

    for v in r.try_slice().unwrap() {
        assert_eq!(*v, 123);
    }
}
