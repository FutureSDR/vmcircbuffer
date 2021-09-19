# Double Mapped Circular Buffer


``` rust
let w = Circular::new::<u32>().unwrap();
let r = w.add_reader();

for v in w.slice() {
    *v = 123;
}
w.produce(w.slice().len());

for v in r.slice().unwrap() {
    assert_eq!(*v, 123);
}
```

