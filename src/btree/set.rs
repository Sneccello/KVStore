use crate::btree::BTree;
use crate::btree::btree_node::{BTreeNode, StorageMeta};
use crate::btree::common::PageId;
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::btree::traits::SerializedSize;
use crate::errors::KvResult;

impl BTree{
    pub fn set(&mut self, key: &[u8], value: &[u8]) -> KvResult<()> {

        self.maybe_split_root(key, value)?;

        let mut curr_page = self.root;

        loop{
            let node = self.page_manager.get_node_mut(curr_page)?;

            match  node{
                BTreeNode::Leaf(node) => {
                    node.set_key_value(key, value);
                    return Ok(())
                }
                BTreeNode::Internal(node) => {
                    let child_idx = node.route_key_to_index(key);
                    self.maybe_split(curr_page, child_idx, key, value)?;

                    let next_page = {
                        let node = self.page_manager
                            .get_node(curr_page)?;

                        node.as_internal().route_key_to_child(key)
                    };
                    curr_page = next_page;
                }
            }

        }
    }

    pub fn maybe_split(&mut self, parent_page: PageId, child_idx: usize,
                       key: &[u8], value: &[u8]) -> KvResult<()>{

        let parent = self.page_manager.get_node(parent_page)?.as_internal();

        let child_page = parent.get_child_by_index(child_idx);
        let child = self.page_manager.get_node(child_page)?;

        if child.is_leaf(){
            let is_full = match child {
                BTreeNode::Leaf(leaf) => {
                    if let Some(old_val) = leaf.get_value_by_key(key) {
                        let old_val_size = old_val.byte_size();
                        let new_val_size = value.byte_size();
                        if new_val_size > old_val_size {
                            leaf.header.total_size_bytes() + (new_val_size - old_val_size) > self.page_size
                        } else {
                            false
                        }
                    } else {
                        key.byte_size() + value.byte_size() + child.total_size_bytes() > self.page_size
                    }
                }
                _ => unreachable!(),
            };
            if is_full {
                self.split_leaf(parent_page, child_page, child_idx)?;
            }
        }else{
            let additional_bytes = key.byte_size() + size_of::<PageId>() as u16;
            if additional_bytes + child.total_size_bytes() > self.page_size {
                self.split_internal(parent_page, child_page, child_idx)?;
            }
        }
        Ok(())

    }

    pub fn maybe_split_root(&mut self, key: &[u8], value: &[u8]) -> KvResult<()> {

        let (is_full, is_leaf) = {
            let root_node = self.page_manager.get_node_mut(self.root)?;

            let is_full = match root_node {
                BTreeNode::Leaf(leaf) => {
                    if let Some(old_val) = leaf.get_value_by_key(key) {
                        let old_val_size = old_val.byte_size();
                        let new_val_size = value.byte_size();
                        if new_val_size > old_val_size {
                            leaf.header.total_size_bytes() + (new_val_size - old_val_size) > self.page_size
                        } else {
                            false
                        }
                    } else {
                        leaf.header.total_size_bytes() + key.byte_size() + value.byte_size() > self.page_size
                    }
                }
                BTreeNode::Internal(internal) => {
                    internal.header.total_size_bytes() + key.byte_size() + (size_of::<PageId>() as u16) > self.page_size
                }
            };

            (is_full, root_node.is_leaf())
        };

        if is_full {
            let old_root_id = self.root;

            let mut new_root = InternalNode::new();
            new_root.push_child(old_root_id);

            let new_root_id = self.page_manager.alloc_node(BTreeNode::Internal(new_root));
            self.root = new_root_id;

            {//TODO why does this block help
                //let old_root = self.page_manager.get_node_mut(old_root_id)?; set root if needed

                if is_leaf {
                    self.split_leaf(new_root_id, old_root_id, 0)?
                } else {
                    self.split_internal(new_root_id, old_root_id, 0)?
                }
            }
        }
        Ok(())
    }

    fn split_internal(&mut self, parent_id: PageId, child_id: PageId, child_idx: usize) -> KvResult<()> {
        //    [5]               [3,5]
        //[1,2,3,4,5] [6] -> [1,2] [3,4,5] [6]

        let (promoted_key, new_node) = {
            let child = self.page_manager.get_node_mut(child_id)?.as_internal_mut();
            let n_keys = child.get_keys().len();

            let (promoted_key, new_node) = if n_keys <= 1 {
                let promoted_key = child.pop_last_key();
                let last_child = child.children.pop().unwrap();
                child.header.items_total_size -= last_child.byte_size();
                let mut new_node = InternalNode::new();
                new_node.push_child(last_child);
                (promoted_key, new_node)
            } else {
                let mut size = (size_of::<StorageMeta>() + size_of::<PageId>()) as u16;
                let mut index = 0;
                let child_keys = child.get_keys();
                while (size < self.page_size_half || index <= 1) && index < n_keys - 1 {
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

        let new_page_id = self.page_manager.alloc_node(BTreeNode::Internal(new_node));

        let parent = self.page_manager.get_node_mut(parent_id)?.as_internal_mut();

        parent.insert_key_child(child_idx, promoted_key, child_idx + 1, new_page_id);
        Ok(())
    }

    fn split_leaf(&mut self, parent_page: PageId, child_page: PageId, child_idx: usize) -> KvResult<()>{

        let (promoted_key, new_node) = {
            let child = self.page_manager.get_node_mut(child_page)?.as_leaf_mut();
            let keys = child.get_keys().len();
            if keys <= 1 {
                let promoted_key = child.get_keys().last().unwrap().clone();
                (promoted_key, LeafNode::new())
            } else {
                let mut size = size_of::<StorageMeta>() as u16;
                let mut index = 0;
                while (size < self.page_size_half || index < 1) && index < keys - 1 {
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
            }
        };

        let new_page_id = self.page_manager.alloc_node(BTreeNode::Leaf(new_node));

        let parent = self.page_manager.get_node_mut(parent_page)?.as_internal_mut() ;

        parent.insert_key_child(child_idx, promoted_key, child_idx + 1, new_page_id);

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use crate::btree::common::PageId;
    use crate::btree::page_manager::PersistentPageManager;
    use crate::btree::test_utils::{get_empty_internal_root, get_empty_leaf_root, new_internal, new_leaf};

    #[test]
    fn test_first_set_in_root() {
        let mut tree = get_empty_leaf_root(64);
        tree.set(b"hello", b"world!").unwrap();
        let root = tree.page_manager.get_node(tree.root).unwrap().as_leaf();
        assert_eq!(root.header.keys_total_size, 5+8);
        assert_eq!(root.header.items_total_size, 6+8);

        assert_eq!(root.keys, vec![b"hello".to_vec()]);
        assert_eq!(root.values, vec![b"world!".to_vec()]);
    }

    #[test]
    fn leaf_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_leaf_root(64);
        let root = tree.page_manager.get_node(tree.root).unwrap().as_leaf();
        let size_left = root.header.total_size_bytes();

        //
        tree.set(b"hi0", b"world").unwrap();
        tree.set(b"hi1", b"world").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);
        //assuming 64-16-2*(7+2*8) = 2 bytes left
        tree.maybe_split_root(b"8longkey", b"3val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);
    }

    #[test]
    fn internal_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_internal_root(96);

        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();

        //80 bytes left
        root.push_lasts(b"k1".to_vec(), 1u64); //2+8+8=18 bytes
        root.push_lasts(b"k2".to_vec(), 2u64); //2+8+8=18 bytes
        root.push_lasts(b"k3".to_vec(), 3u64); //2+8+8=18 bytes
        root.push_child(4u64); //8bytes

        //18 bytes left
        assert_eq!(tree.page_manager.get_pages().len(), 1);
        tree.maybe_split_root(b"k", b"val1").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);

        tree.maybe_split_root(b"k1", b"val-000000001").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);

        tree.maybe_split_root(b"key01", b"v1").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);


        //leaves check
        assert!(!tree.page_manager.get_node(0u64).unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(2u64).unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(1u64).unwrap().is_leaf());
    }

    #[test]
    fn test_internal_node_split(){
        let mut tree = get_empty_leaf_root(64);

        /*
             root [k6]                   root [k1, k6]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k2]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */

        tree.page_manager = Box::new(PersistentPageManager::new());

        let root_page = new_internal(&mut tree.page_manager);
        tree.root = root_page;

        let i1_page = new_internal(&mut tree.page_manager);
        let i2_page = new_internal(&mut tree.page_manager);
        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        root.push_lasts(b"key6".to_vec(), i1_page);
        root.push_child(i2_page);

        let i1 = tree.page_manager.get_node_mut(i1_page).unwrap().as_internal_mut();

        //48bytes left
        i1.push_lasts(b"key0".to_vec(), 00u64);
        i1.push_lasts(b"key1".to_vec(), 10u64);
        i1.push_lasts(b"key2".to_vec(), 20u64);
        i1.push_child(30u64);

        tree.split_internal(tree.root, i1_page, 0).unwrap();

        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        assert_eq!(root.keys, vec![b"key1".to_vec(), b"key6".to_vec()]);
        assert_eq!(root.children.len(), 3);

        let first_child_page = root.get_child_by_index(0);
        let second_child_page = root.get_child_by_index(1);
        let third_child_page = root.get_child_by_index(2);


        assert_eq!(tree.page_manager.get_pages().len()-1, second_child_page as usize);

        let first_child =tree.page_manager.get_node_mut(first_child_page).unwrap().as_internal_mut();
        assert_eq!(first_child.keys, vec![b"key0".to_vec()]);
        assert_eq!(first_child.children, vec![00u64, 10u64]);

        let second_child =tree.page_manager.get_node_mut(second_child_page).unwrap().as_internal_mut();
        assert_eq!(second_child.keys, vec![b"key2".to_vec()]);
        assert_eq!(second_child.children, vec![20u64, 30u64]);

        let third_child =tree.page_manager.get_node_mut(third_child_page).unwrap().as_internal_mut();
        assert_eq!(third_child.keys, Vec::<Vec<u8>>::new());
        assert_eq!(third_child.children, Vec::<PageId>::new());
    }


    #[test]
    fn test_leaf_node_split(){
        let mut tree = get_empty_leaf_root(96);

        /*
             root [k6]                   root [k1, k6]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k2]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */

        tree.page_manager = Box::new(PersistentPageManager::new());

        let root_page = new_internal(&mut tree.page_manager);
        tree.root = root_page;

        let l1_page = new_leaf(&mut tree.page_manager);
        let l2_page = new_leaf(&mut tree.page_manager);
        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        root.push_lasts(b"k6".to_vec(), l1_page);
        root.push_child(l2_page);

        let l1 = tree.page_manager.get_node_mut(l1_page).unwrap().as_leaf_mut();

        //48bytes left
        l1.push_lasts(b"k0".to_vec(), b"v0".to_vec());
        l1.push_lasts(b"k1".to_vec(), b"v1".to_vec());
        l1.push_lasts(b"k2".to_vec(), b"v2".to_vec());
        l1.push_lasts(b"k3".to_vec(), b"v3".to_vec());

        tree.split_leaf(tree.root, l1_page, 0).unwrap();

        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        assert_eq!(root.keys, vec![b"k1".to_vec(), b"k6".to_vec()]);
        assert_eq!(root.children.len(), 3);

        let first_leaf_page = root.get_child_by_index(0);
        let second_leaf_page = root.get_child_by_index(1);
        let third_leaf_page = root.get_child_by_index(2);


        assert_eq!(tree.page_manager.get_pages().len()-1, second_leaf_page as usize);

        let first_leaf =tree.page_manager.get_node_mut(first_leaf_page).unwrap().as_leaf_mut();
        assert_eq!(first_leaf.keys, vec![b"k0".to_vec(), b"k1".to_vec()]);
        assert_eq!(first_leaf.values, vec![b"v0".to_vec(), b"v1".to_vec()]);

        let second_leaf =tree.page_manager.get_node_mut(second_leaf_page).unwrap().as_leaf_mut();
        assert_eq!(second_leaf.keys, vec![b"k2".to_vec(), b"k3".to_vec()]);
        assert_eq!(second_leaf.values, vec![b"v2".to_vec(), b"v3".to_vec()]);

        let third_leaf =tree.page_manager.get_node_mut(third_leaf_page).unwrap().as_leaf_mut();
        assert_eq!(third_leaf.keys, Vec::<Vec<u8>>::new());
        assert_eq!(third_leaf.values, Vec::<Vec<u8>>::new());
    }
}