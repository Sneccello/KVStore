mod common;

use kv_store::engine::StorageEngine;
use kv_store::errors::KvError;
use crate::common::utils::{insert_keys_values, new_tree, shuffle};

#[test]
fn test_btree_insert() {
    let mut tree = new_tree(96);

    tree.set(b"key1", b"value1").unwrap();
    tree.set(b"key2", b"value2").unwrap();

    let res = tree.get(b"key1").unwrap();
    assert_eq!(res, Some(b"value1".to_vec()));

    let res = tree.get(b"key2").unwrap();
    assert_eq!(res, Some(b"value2".to_vec()));
}


#[test]
fn test_btree_overwrite(){
    let mut tree = new_tree(96);

    tree.set(b"key1", b"value1").unwrap();
    tree.set(b"key1", b"value2").unwrap();

    let res = tree.get(b"key1").unwrap();
    assert_eq!(res, Some(b"value2".to_vec()));
}

#[test]
fn test_btree_delete(){
    let mut tree = new_tree(96);

    tree.set(b"key1", b"value1").unwrap();
    tree.delete(b"key1").unwrap();

    let res = tree.get(b"key1").unwrap();

    assert_eq!(res, None);
}

#[test]
fn test_btree_delete_non_existing(){
    let mut tree = new_tree(96);

    tree.set(b"key1", b"value1").unwrap();
    tree.delete(b"key1").unwrap();

    let res = tree.delete(b"key1");
    assert!(matches!(
        res,
        Err(KvError::KeyNotFound(ref k)) if k == b"key1"
    ));

    let res = tree.delete(b"key2");
    assert!(matches!(
        res,
        Err(KvError::KeyNotFound(ref k)) if k == b"key2"
    ));
}


#[test]
fn test_btree_delete_interleaved(){
    let mut tree = new_tree(96);

    let items = 100;
    let keys = (0..items).map(|i| i.to_string().into_bytes()).collect::<Vec<_>>();
    let values = (0..items).map(|i| i.to_string().into_bytes()).collect::<Vec<_>>();
    println!("inserting {items} values");
    insert_keys_values(&mut tree, &keys, &values);

    for index in (0..items).step_by(2) {
        println!("getting {:?}", &keys[index]);
        let res = tree.get(&keys[index]).unwrap();
        println!("got {:?}", res);
        assert_eq!(res, Some(values[index].to_vec()));

        println!("deleting {:?}", &keys[index]);
        tree.delete(&keys[index]).unwrap();
        let res = tree.get(&keys[index]).unwrap();
        assert_eq!(res, None)
    }
    for index in (1..items).step_by(2) {
        println!("re-getting {:?}", &keys[index]);
        let res = tree.get(&keys[index]).unwrap();
        assert_eq!(res, Some(values[index].to_vec()));

    }
}

#[test]
fn test_btree_delete_reversed(){
    let mut tree = new_tree(96);

    let items = 100;
    let keys = (0..items)
        .map(|i| format!("{:04}", i).into_bytes())
        .collect::<Vec<_>>();

    let values = (0..items).map(|i| i.to_string().into_bytes()).collect::<Vec<_>>();

    insert_keys_values(&mut tree, &keys, &values);

    for index in (0..items).step_by(2) {
        let rev_index = items - 1 - index;
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, Some(values[rev_index].to_vec()));

        tree.delete(&keys[rev_index]).unwrap();
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, None)
    }
    println!("{:?}", keys);
    println!("{:?}", values);
    for index in (1..items).step_by(2) {
        let rev_index = items - 1 - index;
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, Some(values[rev_index].to_vec()));
    }
}
#[test]
fn test_btree_delete_random(){
    let mut tree = new_tree(96);

    let items = 100;
    let mut keys = (0..items)
        .map(|i| format!("{:04}", i).into_bytes())
        .collect::<Vec<_>>();

    shuffle(&mut keys);

    let values = (0..items).map(|i| i.to_string().into_bytes()).collect::<Vec<_>>();


    insert_keys_values(&mut tree, &keys, &values);

    for index in (0..items).step_by(2) {
        let rev_index = items - 1 - index;
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, Some(values[rev_index].to_vec()));

        tree.delete(&keys[rev_index]).unwrap();
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, None)
    }

    for index in (1..items).step_by(2) {
        let rev_index = items - 1 - index;
        let res = tree.get(&keys[rev_index]).unwrap();
        assert_eq!(res, Some(values[rev_index].to_vec()));
    }
}

