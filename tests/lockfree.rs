use std::iter::repeat_with;

use vmcircbuffer::generic::{Metadata, NoMetadata};
use vmcircbuffer::lockfree::{Circular, CircularError};

#[test]
fn create_many() {
    let mut v = Vec::new();
    for _ in 0..100 {
        v.push(Circular::new::<u8, NoMetadata>(4).unwrap());
    }
}

#[test]
fn zero_size() {
    let mut w = Circular::new::<u8, NoMetadata>(1).unwrap();
    assert!(!w.slice().is_empty());
}

#[test]
fn no_reader() {
    let mut w = Circular::new::<u8, NoMetadata>(1).unwrap();
    let s = w.slice();
    let l = s.len();
    w.produce(l, &[]);
    assert!(!w.slice().is_empty());
}

#[test]
#[should_panic]
fn produce_too_much() {
    let mut w = Circular::new::<u8, NoMetadata>(1).unwrap();
    let s = w.slice();
    let l = s.len();
    w.produce(l + 1, &[]);
}

#[test]
#[should_panic]
fn consume_too_much() {
    let mut w = Circular::new::<u8, NoMetadata>(1).unwrap();
    let mut r = w.add_reader().unwrap();
    let mut tags = Vec::new();
    let s = w.slice();
    let l = s.len();
    w.produce(l + 1, &[]);
    let s = r.slice_with_meta_into(&mut tags).unwrap();
    let l = s.len();
    r.consume(l + 1);
}

#[test]
fn late_reader() {
    let mut w = Circular::new::<u32, NoMetadata>(1).unwrap();
    let s = w.slice();
    for (i, v) in s.iter_mut().take(200).enumerate() {
        *v = i as u32;
    }
    w.produce(100, &[]);

    let mut r = w.add_reader().unwrap();
    let mut tags = Vec::new();
    w.produce(100, &[]);
    let s = r.slice_with_meta_into(&mut tags).unwrap();
    assert_eq!(s.len(), 100);
    for (i, v) in s.iter().enumerate() {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn several_readers() {
    let mut w = Circular::new::<u32, NoMetadata>(2).unwrap();

    let mut r1 = w.add_reader().unwrap();
    let mut r2 = w.add_reader().unwrap();
    let mut tags1 = Vec::new();
    let mut tags2 = Vec::new();

    for (i, v) in w.slice().iter_mut().enumerate() {
        *v = i as u32;
    }
    let all = w.slice().len();
    w.produce(all, &[]);

    assert_eq!(r2.slice_with_meta_into(&mut tags2).unwrap().len(), all);
    assert_eq!(r1.slice_with_meta_into(&mut tags1).unwrap().len(), all);

    let _ = r1.slice_with_meta_into(&mut tags1);
    r1.consume(100);

    assert_eq!(
        r1.slice_with_meta_into(&mut tags1).unwrap().len(),
        all - 100
    );
    for (i, v) in r1
        .slice_with_meta_into(&mut tags1)
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(*v, 100 + i as u32);
    }
}

#[test]
fn max_readers_exceeded() {
    let w = Circular::new::<u8, NoMetadata>(1).unwrap();
    let _r1 = w.add_reader().unwrap();
    match w.add_reader() {
        Err(CircularError::TooManyReaders) => {}
        Err(_) => panic!("unexpected error"),
        Ok(_) => panic!("expected TooManyReaders error"),
    }
}

#[test]
fn block_writer() {
    let mut w = Circular::new::<f32, NoMetadata>(1).unwrap();
    let mut r = w.add_reader().unwrap();
    let mut tags = Vec::new();

    let l = w.slice().len();
    w.produce(l, &[]);

    // buffer full, writer sees no space
    assert!(w.slice().is_empty());

    let l = r.slice_with_meta_into(&mut tags).unwrap().len();
    r.consume(l);
    assert!(!w.slice().is_empty());
}

#[test]
fn block_reader() {
    let mut w = Circular::new::<f32, NoMetadata>(1).unwrap();
    let mut r = w.add_reader().unwrap();
    let mut tags = Vec::new();

    // no data yet
    assert!(r.slice_with_meta_into(&mut tags).unwrap().is_empty());

    let l = w.slice().len();
    w.produce(l, &[]);
    assert!(!r.slice_with_meta_into(&mut tags).unwrap().is_empty());
}

#[test]
fn fuzz_lockfree() {
    let mut w = Circular::new::<u32, NoMetadata>(1).unwrap();
    let r = w.add_reader().unwrap();

    let input: Vec<u32> = repeat_with(rand::random::<u32>).take(123123).collect();
    let input = std::sync::Arc::new(input);

    let input_w = input.clone();
    let producer = std::thread::spawn(move || {
        let mut w_off = 0;
        while w_off < input_w.len() {
            let s = w.slice();
            if s.is_empty() {
                continue;
            }
            let n = std::cmp::min(s.len(), input_w.len() - w_off);
            for (i, v) in s.iter_mut().take(n).enumerate() {
                *v = input_w[w_off + i];
            }
            w.produce(n, &[]);
            w_off += n;
        }
    });

    let consumer = std::thread::spawn(move || {
        let mut r = r;
        let mut tags = Vec::new();
        let mut r_off = 0;
        while r_off < input.len() {
            let s = r.slice_with_meta_into(&mut tags).unwrap();
            let n = s.len();
            if n == 0 {
                continue;
            }
            for (i, v) in s.iter().enumerate() {
                assert_eq!(*v, input[r_off + i]);
            }
            r.consume(n);
            r_off += n;
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}

#[derive(Clone)]
struct Tag {
    item: usize,
    data: String,
}

struct MyMetadata {
    tags: Vec<Tag>,
}

impl Metadata for MyMetadata {
    type Item = Tag;

    fn new() -> Self {
        MyMetadata { tags: Vec::new() }
    }
    fn add_from_slice(&mut self, offset: usize, tags: &[Self::Item]) {
        for t in tags {
            let mut t = t.clone();
            t.item += offset;
            self.tags.push(t);
        }
    }
    fn get_into(&self, out: &mut Vec<Self::Item>) {
        out.clear();
        out.extend(self.tags.iter().cloned());
    }
    fn consume(&mut self, items: usize) {
        self.tags.retain(|x| x.item >= items);
        for t in self.tags.iter_mut() {
            t.item -= items;
        }
    }
}

#[test]
fn tags() {
    let mut w = Circular::with_capacity::<u32, MyMetadata>(1, 1).unwrap();
    let mut r = w.add_reader().unwrap();

    let out = w.slice();
    for v in out.iter_mut() {
        *v = 123;
    }
    let len = out.len();

    let tags = vec![
        Tag {
            item: 0,
            data: String::from("first"),
        },
        Tag {
            item: 10,
            data: String::from("tenth"),
        },
    ];
    w.produce(len, &tags);

    let mut tags = Vec::new();
    let i = r.slice_with_meta_into(&mut tags).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].data, String::from("first"));
    assert_eq!(tags[0].item, 0);
    assert_eq!(tags[1].data, String::from("tenth"));
    assert_eq!(tags[1].item, 10);

    r.consume(5);
    let i = r.slice_with_meta_into(&mut tags).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].data, String::from("tenth"));
    assert_eq!(tags[0].item, 5);
}
