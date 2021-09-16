use std::iter::repeat_with;
use rand::distributions::{Distribution, Uniform};

use double_mapped_circular_buffer::Circular;

#[test]
fn create_many() {
    let mut v = Vec::new();
    for _ in 0..100 {
        v.push(Circular::new::<u8>(123).unwrap());
    }
}

#[test]
fn zero_size() {
    let w = Circular::new::<u8>(0).unwrap();
    assert!(w.slice().len() > 0);
}

#[test]
fn no_reader() {
    let w = Circular::new::<u8>(123).unwrap();
    let s =  w.slice();
    w.produce(s.len());
    assert!(w.slice().len() > 0);
}

#[test]
fn late_reader() {
    let w = Circular::new::<u32>(200).unwrap();
    let s = w.slice();
    for (i, v) in s.iter_mut().take(200).enumerate() {
        *v = i as u32;
    }
    w.produce(100);

    let r = w.add_reader();
    assert_eq!(r.slice().len(), 0);
    w.produce(100);
    assert_eq!(r.slice().len(), 100);
    for (i, v) in r.slice().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn several_readers() {
    let w = Circular::new::<u32>(200).unwrap();

    let r1 = w.add_reader();
    let r2 = w.add_reader();

    for (i, v) in w.slice().iter_mut().enumerate() {
        *v = i as u32;
    }
    let all = w.slice().len();
    assert_eq!(r1.slice().len(), 0);
    w.produce(w.slice().len());
    assert_eq!(r2.slice().len(), all);

    r1.consume(100);

    assert_eq!(r1.slice().len(), all - 100);
    for (i, v) in r1.slice().iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn fuzz() {
    let w = Circular::new::<u32>(200).unwrap();
    let r = w.add_reader();
    let size = w.slice().len();

    let input : Vec<u32> = repeat_with(rand::random::<u32>).take(1231233).collect();

    let mut rng = rand::thread_rng();
    let n_writes_dist = Uniform::from(0..4);
    let n_samples_dist = Uniform::from(0..size/2);

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

        let s = r.slice();
        assert_eq!(s.len(), w_off - r_off);

        for (i, v) in s.iter().enumerate() {
            assert_eq!(*v, input[r_off + i]);
        }
        r.consume(s.len());
        r_off += s.len();
    }
}
