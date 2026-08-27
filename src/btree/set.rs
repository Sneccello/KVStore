use std::sync::{Arc, RwLock, RwLockWriteGuard};
use crate::btree::page_managers::page_manager::PageManager;
use crate::btree::BTree;
use crate::btree::btree_node::{BTreeNode, StorageMeta};
use crate::btree::common::PageId;
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::btree::traits::SerializedSize;
use crate::errors::KvResult;
use crate::errors::KvError::{LockError, TreeLogicError};

impl BTree{


    pub fn set(&self, key: &[u8], value: &[u8]) -> KvResult<()> {
        let mut root_guard = self.root.write().map_err(|_| LockError())?;
        {
            let node_arc = self.page_manager.get_node(*root_guard)?;
            let mut node_guard = node_arc.write().map_err(|_| LockError())?;

            self.maybe_split_root(&mut node_guard ,&mut root_guard, key, value)?;
        };

        let current_arc =  self.page_manager.get_node(*root_guard)?;
        let current_guard = current_arc.write().map_err(|_| LockError())?;
        drop(root_guard);

        self.recursive_set(key, value, current_guard)
    }

    pub fn recursive_set(&self, key: &[u8], value: &[u8],
                         mut node_guard: RwLockWriteGuard<BTreeNode>,

    ) -> KvResult<()> {

        match &mut *node_guard{
            BTreeNode::Leaf(node) => {
                node.set_key_value(key, value);
                Ok(())
            }
            BTreeNode::Internal(node) => {
                let child_idx = node.route_key_to_index(key);
                let mut child_page = node.get_child_by_index(child_idx);
                let mut child_arc = self.page_manager.get_node(child_page)?;
                let mut child_guard = child_arc.write().map_err(|_| LockError())?;
                let split_happened = self.maybe_split(&mut node_guard, &mut child_guard, child_idx, key, value)?;
                let node = (*node_guard).as_internal();
                if split_happened{
                    drop(child_guard);
                    child_page = node.route_key_to_child(key);
                    child_arc = self.page_manager.get_node(child_page)?;
                    child_guard = child_arc.write().map_err(|_| LockError())?;
                }

                drop(node_guard);

                self.recursive_set(key, value, child_guard)
            }
        }
    }

    pub fn maybe_split(&self, parent: &mut RwLockWriteGuard<BTreeNode>,
                       child: &mut RwLockWriteGuard<BTreeNode>,
                       child_idx: usize, key: &[u8], value: &[u8]) -> KvResult<(bool)>{


        if child.is_leaf(){
            let is_full = match &mut **child {
                BTreeNode::Leaf(leaf) => {
                    if let Some(old_val) = leaf.get_value_by_key(key) {
                        let old_val_size = old_val.byte_size();
                        let new_val_size = value.byte_size();
                        if new_val_size > old_val_size {
                            leaf.header.total_size_bytes() + (new_val_size - old_val_size) > self.node_fat_limit_bytes
                        } else {
                            false
                        }
                    } else {
                        key.byte_size() + value.byte_size() + child.total_size_bytes() > self.node_fat_limit_bytes
                    }
                }
                _ => unreachable!(),
            };
            if is_full {
                self.split_leaf(parent, child, child_idx)?;
                return Ok(true);
            }
        }else{
            let additional_bytes = key.byte_size() + size_of::<PageId>() as u16;
            if additional_bytes + child.total_size_bytes() > self.node_fat_limit_bytes {
                self.split_internal(parent, child, child_idx)?;
                return Ok(true)
            }
        }
        Ok(false)

    }

    pub fn maybe_split_root(&self, root_node: &mut RwLockWriteGuard<BTreeNode>,
                            root_guard: &mut RwLockWriteGuard<PageId>, key: &[u8], value: &[u8]) -> KvResult<()> {

        let (is_full, is_leaf) = {

            let is_full = match &**root_node {
                BTreeNode::Leaf(leaf) => {
                    if let Some(old_val) = leaf.get_value_by_key(key) {
                        let old_val_size = old_val.byte_size();
                        let new_val_size = value.byte_size();
                        if new_val_size > old_val_size {
                            leaf.header.total_size_bytes() + (new_val_size - old_val_size) > self.node_fat_limit_bytes
                        } else {
                            false
                        }
                    } else {
                        leaf.header.total_size_bytes() + key.byte_size() + value.byte_size() > self.node_fat_limit_bytes
                    }
                }
                BTreeNode::Internal(internal) => {
                    internal.header.total_size_bytes() + key.byte_size() + (size_of::<PageId>() as u16) > self.node_fat_limit_bytes
                }
            };

            (is_full, root_node.is_leaf())
        };

        if is_full {
            let old_root_id = (**root_guard).clone();
            let mut new_root = InternalNode::new();
            new_root.push_child(old_root_id);

            let new_root_id = self.page_manager.alloc_node(BTreeNode::Internal(new_root))?;
            let new_root_guard = self.page_manager.get_node(new_root_id)?;
            let mut new_root = new_root_guard.write().map_err(|_| LockError())?;
            **root_guard = new_root_id;

            {//TODO why does this block help
                //let old_root = pm.get_node_mut(old_root_id)?; set root if needed

                if is_leaf {
                    self.split_leaf(&mut new_root, root_node, 0)?
                } else {
                    self.split_internal(&mut new_root, root_node, 0)?
                }
            }
        }
        Ok(())
    }

    fn split_internal(&self, parent: &mut RwLockWriteGuard<BTreeNode>, child: &mut RwLockWriteGuard<BTreeNode>,
                      child_idx: usize) -> KvResult<()> {
        //    [5]               [3,5]
        //[1,2,3,4,5] [6] -> [1,2] [3,4,5] [6]

        let mut child = child.as_internal_mut();
        let (promoted_key, new_node) = {
            let n_keys = child.get_keys().len();

            if n_keys <= 1 {
                return Err(TreeLogicError("Page size too small to split internal node".into()));
            }

            let (promoted_key, new_node) = {
                let mut size = (size_of::<StorageMeta>() + size_of::<PageId>()) as u16;
                let mut index = 0;
                let child_keys = child.get_keys();
                while (size < self.node_thin_limit_bytes || index <= 1) && index < n_keys - 1 {
                    size += child_keys[index].byte_size() + (size_of::<PageId>() as u16);
                    index += 1;
                }
                let index = index.max(1);

                let mut new_node = InternalNode::new();
                let (keys, values) = &mut child.split_off(index);

                new_node.append(keys, values);

                let promoted_key = child.pop_last_key();

                (promoted_key, new_node)
            };

            (promoted_key, new_node)
        };

        let new_page_id = self.page_manager.alloc_node(BTreeNode::Internal(new_node))?;

        let parent = parent.as_internal_mut();

        parent.insert_key_child(child_idx, promoted_key, child_idx + 1, new_page_id);
        Ok(())
    }

    fn split_leaf(&self, parent: &mut RwLockWriteGuard<BTreeNode>,
                  child: &mut RwLockWriteGuard<BTreeNode>, child_idx: usize) -> KvResult<()>{

        let child = child.as_leaf_mut();
        let keys = child.get_keys().len();
        if keys <= 1 {
            return Err(TreeLogicError("Key-value pair exceeds maximum page size".into()));
        }
        let (promoted_key, new_node) = {
            let mut size = size_of::<StorageMeta>() as u16;
            let mut index = 0;
            while (size < self.node_thin_limit_bytes || index < 1) && index < keys - 1 {
                let (key, value) = child.get_key_value_by_index(index);
                size += key.byte_size();
                size += value.byte_size();
                index += 1;
            }

            let mut new_node = LeafNode::new();
            let (keys, values) = &mut child.split_off(index);
            new_node.append(keys, values);

            let promoted_key = child.get_keys().last().unwrap().clone();
            (promoted_key, new_node)
        };

        let new_page_id = self.page_manager.alloc_node(BTreeNode::Leaf(new_node))?;

        parent.as_internal_mut().insert_key_child(child_idx, promoted_key, child_idx + 1, new_page_id);

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use crate::btree::test_utils;
    use crate::btree::common::PageId;
    use crate::btree::test_utils::{get_empty_internal_root, get_empty_leaf_root, get_root_page, new_internal, new_leaf};

    #[test]
    fn test_first_set_in_root() {
        let mut tree = get_empty_leaf_root(64);
        tree.set(b"hello", b"world!").unwrap();

        let root_node = tree.page_manager.get_node(test_utils::get_root_page(&tree)).unwrap();
        let guard = root_node.read().unwrap();
        let root = guard.as_leaf();

        assert_eq!(root.header.keys_total_size, 5 + 8);
        assert_eq!(root.header.items_total_size, 6 + 8);
        assert_eq!(root.keys, vec![b"hello".to_vec()]);
        assert_eq!(root.values, vec![b"world!".to_vec()]);
    }

    #[test]
    fn leaf_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_leaf_root(64);

        tree.set(b"hi0", b"world").unwrap();
        tree.set(b"hi1", b"world").unwrap();

        let root_page = get_root_page(&tree);
        let mut root_guard = tree.root.write().unwrap();
        let root_node = tree.page_manager.get_node(root_page).unwrap();
        let mut guard = root_node.write().unwrap();
        let size_left = guard.as_leaf().header.total_size_bytes();

        assert_eq!(tree.page_manager.get_pages().len(), 1);
        //assuming 64-16-2*(7+2*8) = 2 bytes left
        tree.maybe_split_root(&mut guard, &mut root_guard, b"8longkey", b"3val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);
    }

    #[test]
    fn internal_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_internal_root(96);

        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root = root_guard.as_internal_mut();
            //80 bytes left
            root.push_lasts(b"k1".to_vec(), 1u64); //2+8+8=18 bytes
            root.push_lasts(b"k2".to_vec(), 2u64); //2+8+8=18 bytes
            root.push_lasts(b"k3".to_vec(), 3u64); //2+8+8=18 bytes
            root.push_child(4u64); //8bytes
        }

        assert_eq!(tree.page_manager.get_pages().len(), 1);


        //18 bytes left
        {
            let root_page = get_root_page(&tree);
            let mut root_guard = tree.root.write().unwrap();
            let root_arc = tree.page_manager.get_node(root_page).unwrap();
            let mut root = root_arc.write().unwrap();
            tree.maybe_split_root(&mut root, &mut root_guard, b"k", b"val1").unwrap();
            assert_eq!(tree.page_manager.get_pages().len(), 1);
        }

        {
            let root_page = get_root_page(&tree);
            let mut root_guard = tree.root.write().unwrap();
            let root_lock = tree.page_manager.get_node(root_page).unwrap();
            let mut root = root_lock.write().unwrap();
            tree.maybe_split_root(&mut root, &mut root_guard, b"k1", b"val-000000001").unwrap();
            assert_eq!(tree.page_manager.get_pages().len(), 1);
        }

        {
            let root_page = get_root_page(&tree);
            let mut root_guard = tree.root.write().unwrap();
            let root_lock = tree.page_manager.get_node(root_page).unwrap();
            let mut root = root_lock.write().unwrap();
            tree.maybe_split_root(&mut root, &mut root_guard, b"key01", b"v1").unwrap();
            assert_eq!(tree.page_manager.get_pages().len(), 3);
        }


        //leaves check
        assert!(!tree.page_manager.get_node(0u64).unwrap().read().unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(2u64).unwrap().read().unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(1u64).unwrap().read().unwrap().is_leaf());
    }

    #[test]
    fn test_internal_node_split(){
        let mut tree = get_empty_internal_root(64);

        /*
             root [k6]                   root [k1, k6]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k2]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */


        let i1_page = new_internal(&mut tree.page_manager);
        let i2_page = new_internal(&mut tree.page_manager);
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root_node = root_guard.as_internal_mut();
            root_node.push_lasts(b"key6".to_vec(), i1_page);
            root_node.push_child(i2_page);
        }
        {
            let i1_lock = tree.page_manager.get_node(i1_page).unwrap();
            let mut i1_guard = i1_lock.write().unwrap();
            let i1 = i1_guard.as_internal_mut();
            //48bytes left
            i1.push_lasts(b"key0".to_vec(), 00u64);
            i1.push_lasts(b"key1".to_vec(), 10u64);
            i1.push_lasts(b"key2".to_vec(), 20u64);
            i1.push_child(30u64);
        }
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let i1_lock = tree.page_manager.get_node(i1_page).unwrap();
            let mut i1_guard = i1_lock.write().unwrap();

            tree.split_internal(&mut root_guard, &mut i1_guard, 0).unwrap();
        }

        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root = root_guard.as_internal();
            assert_eq!(root.keys, vec![b"key1".to_vec(), b"key6".to_vec()]);
            assert_eq!(root.children.len(), 3);



            let first_child_page = root.get_child_by_index(0);
            let second_child_page = root.get_child_by_index(1);
            let third_child_page = root.get_child_by_index(2);

            assert_eq!(tree.page_manager.get_pages().len()-1, second_child_page as usize);

            let first_child_lock = tree.page_manager.get_node(first_child_page).unwrap();
            let first_child_guard = first_child_lock.read().unwrap();
            let first_child = first_child_guard.as_internal();
            assert_eq!(first_child.keys, vec![b"key0".to_vec()]);
            assert_eq!(first_child.children, vec![00u64, 10u64]);

            let second_child_lock = tree.page_manager.get_node(second_child_page).unwrap();
            let second_child_guard = second_child_lock.read().unwrap();
            let second_child = second_child_guard.as_internal();
            assert_eq!(second_child.keys, vec![b"key2".to_vec()]);
            assert_eq!(second_child.children, vec![20u64, 30u64]);

            let third_child_lock = tree.page_manager.get_node(third_child_page).unwrap();
            let third_child_guard = third_child_lock.read().unwrap();
            let third_child = third_child_guard.as_internal();
            assert_eq!(third_child.keys, Vec::<Vec<u8>>::new());
            assert_eq!(third_child.children, Vec::<PageId>::new());
        }
    }


    #[test]
    fn test_leaf_node_split(){
        let mut tree = get_empty_internal_root(96);

        /*
             root [k6]                   root [k1, k6]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k2]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */


        let l1_page = new_leaf(&mut tree.page_manager);
        let l2_page = new_leaf(&mut tree.page_manager);
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let mut root_node = root_guard.as_internal_mut();
            root_node.push_lasts(b"k6".to_vec(), l1_page);
            root_node.push_child(l2_page);
        }

        {
            let l1_lock = tree.page_manager.get_node(l1_page).unwrap();
            let mut l1_guard = l1_lock.write().unwrap();
            let l1 = l1_guard.as_leaf_mut();
            //48bytes left
            l1.push_lasts(b"k0".to_vec(), b"v0".to_vec());
            l1.push_lasts(b"k1".to_vec(), b"v1".to_vec());
            l1.push_lasts(b"k2".to_vec(), b"v2".to_vec());
            l1.push_lasts(b"k3".to_vec(), b"v3".to_vec());
        }
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();

            let l1_lock = tree.page_manager.get_node(l1_page).unwrap();
            let mut l1_guard = l1_lock.write().unwrap();
            tree.split_leaf(&mut root_guard, &mut l1_guard, 0).unwrap();
        }

        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root = root_guard.as_internal();
            assert_eq!(root.keys, vec![b"k1".to_vec(), b"k6".to_vec()]);
            assert_eq!(root.children.len(), 3);

            let first_leaf_page = root.get_child_by_index(0);
            let second_leaf_page = root.get_child_by_index(1);
            let third_leaf_page = root.get_child_by_index(2);

            assert_eq!(tree.page_manager.get_pages().len()-1, second_leaf_page as usize);

            let first_leaf_lock = tree.page_manager.get_node(first_leaf_page).unwrap();
            let first_leaf_guard = first_leaf_lock.read().unwrap();
            let first_leaf = first_leaf_guard.as_leaf();
            assert_eq!(first_leaf.keys, vec![b"k0".to_vec(), b"k1".to_vec()]);
            assert_eq!(first_leaf.values, vec![b"v0".to_vec(), b"v1".to_vec()]);

            let second_leaf_lock = tree.page_manager.get_node(second_leaf_page).unwrap();
            let second_leaf_guard = second_leaf_lock.read().unwrap();
            let second_leaf = second_leaf_guard.as_leaf();
            assert_eq!(second_leaf.keys, vec![b"k2".to_vec(), b"k3".to_vec()]);
            assert_eq!(second_leaf.values, vec![b"v2".to_vec(), b"v3".to_vec()]);

            let third_leaf_lock = tree.page_manager.get_node(third_leaf_page).unwrap();
            let third_leaf_guard = third_leaf_lock.read().unwrap();
            let third_leaf = third_leaf_guard.as_leaf();
            assert_eq!(third_leaf.keys, Vec::<Vec<u8>>::new());
            assert_eq!(third_leaf.values, Vec::<Vec<u8>>::new());
        }

    }
}