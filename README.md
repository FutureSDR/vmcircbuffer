# Double Mapped Circular Buffer

This circular buffer implementation maps the underlying buffer twice,
back-to-back into the virtual address space of the process. This arrangement
allows the circular buffer to present the available data sequentially, (i.e., as
a slice) without having to worry about wrapping.

[![Crates.io][crates-badge]][crates-url]
[![Apache 2.0 licensed][apache-badge]][apache-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/vmcircbuffer.svg
[crates-url]: https://crates.io/crates/vmcircbuffer
[apache-badge]: https://img.shields.io/badge/license-Apache%202-blue
[apache-url]: https://github.com/futuresdr/vmcircbuffer/blob/main/LICENSE
[actions-badge]: https://github.com/futuresdr/vmcircbuffer/workflows/CI/badge.svg
[actions-url]: https://github.com/futuresdr/vmcircbuffer/actions?query=workflow%3ACI+branch%3Amain

```rust
# use vmcircbuffer::sync;
# use vmcircbuffer::generic::CircularError;
let mut w = sync::Circular::new::<u32>()?;
let mut r = w.add_reader();

// delay producing by 1 sec
let now = std::time::Instant::now();
let delay = std::time::Duration::from_millis(1000);

// producer thread
std::thread::spawn(move || {
    std::thread::sleep(delay);
    let w_buff = w.slice();
    for v in w_buff.iter_mut() {
        *v = 23;
    }
    let l = w_buff.len();
    w.produce(l);
});

// blocks until data becomes available
let r_buff = r.slice().unwrap();
assert!(now.elapsed() > delay);
for v in r_buff {
    assert_eq!(*v, 23);
}
let l = r_buff.len();
r.consume(l);
# Ok::<(), CircularError>(())
```
