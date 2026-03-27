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

fn main() {
    let mut w = Circular::with_capacity::<u32, MyNotifier, MyMetadata>(1).unwrap();

    let mut r = w.add_reader(MyNotifier, MyNotifier);

    let out = w.slice(false);
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
    let i = r.slice_with_metadata_into(false, &mut tags).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].data, String::from("first"));
    assert_eq!(tags[0].item, 0);
    assert_eq!(tags[1].data, String::from("tenth"));
    assert_eq!(tags[1].item, 10);

    r.consume(5);
    let i = r.slice_with_metadata_into(false, &mut tags).unwrap();

    assert_eq!(i[0], 123);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].data, String::from("tenth"));
    assert_eq!(tags[0].item, 5);
}
