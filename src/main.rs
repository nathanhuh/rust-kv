mod store;
use std::path::Path;
use store::KvStore;

fn main() {
    let mut test_store = KvStore::open(Path::new("test.log")).unwrap();

    test_store
        .set(String::from("name"), String::from("nathan"))
        .unwrap();
    let get_result = match test_store.get(String::from("name")).unwrap() {
        Some(x) => x,
        None => panic!(),
    };
    println!("{}", get_result);
}
