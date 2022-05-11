use vmcircbuffer::generic::Circular;
use vmcircbuffer::generic::Metadata;
use vmcircbuffer::generic::Notifier;

struct MyNotifier;

impl Notifier for MyNotifier {
    fn arm(&mut self) {}
    fn notify(&mut self) {}
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
    fn add(&mut self, offset: usize, mut tags: Vec<Self::Item>) {
        for t in tags.iter_mut() {
            t.item += offset;
        }
        self.tags.append(&mut tags);
    }
    fn get(&self) -> Vec<Self::Item> {
        self.tags.clone()
    }
    fn consume(&mut self, items: usize) {
        self.tags.retain(|x| x.item >= items);
        for t in self.tags.iter_mut() {
            t.item -= items;
        }
    }
}

fn main() {
    let mut w = Circular::with_capacity::<u32, MyNotifier, MyMetadata>(1).unwrap();

    let mut r = w.add_reader(MyNotifier, MyNotifier);

    let out = w.slice(false);
    for v in out.iter_mut() {
        *v = 123;
    }
    let len = out.len();

    w.produce(
        len,
        vec![
            Tag {
                item: 0,
                data: String::from("first"),
            },
            Tag {
                item: 10,
                data: String::from("tenth"),
            },
        ],
    );

    let (i, tags) = r.slice(false).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].data, String::from("first"));
    assert_eq!(tags[0].item, 0);
    assert_eq!(tags[1].data, String::from("tenth"));
    assert_eq!(tags[1].item, 10);

    r.consume(5);
    let (i, tags) = r.slice(false).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].data, String::from("tenth"));
    assert_eq!(tags[0].item, 5);
}
