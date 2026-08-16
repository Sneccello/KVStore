use crate::btree::BTree;
use crate::btree::btree_node::{BTreeNode, StorageMeta};
use crate::btree::common::PageId;
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::btree::traits::ByteSized;
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

        //TODO lift
        if child.is_leaf(){
            let additional_bytes = key.len() + value.len();
            if additional_bytes + child.total_size_bytes() > self.page_size {
                self.split_leaf(parent_page, child_page, child_idx)?;
            }
        }else{
            let additional_bytes = key.len() + size_of::<PageId>();
            if additional_bytes + child.total_size_bytes() > self.page_size {
                self.split_internal(parent_page, child_page, child_idx)?;
            }
        }
        Ok(())

    }

    pub fn maybe_split_root(&mut self, key: &[u8], value: &[u8]) -> KvResult<()> {

        let (is_full, is_leaf) = {
            let root_node = self.page_manager.get_node_mut(self.root)?;

            let new_bytes = if root_node.is_leaf(){
                key.len() + value.len()
            }else{
                key.len() + size_of::<PageId>()
            };


            let is_full = root_node.total_size_bytes() + new_bytes > self.page_size;

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
            let child =  self.page_manager.get_node_mut(child_id)?.as_internal_mut();

            let mut size = size_of::<StorageMeta>() + size_of::<PageId>();
            let mut index = 0;

            while size < self.page_size_half {
                size += child.get_keys()[index].len() + size_of::<PageId>();
                index += 1;
            }

            let mut new_node = InternalNode::new();
            let (keys, values) = &mut child.split_off(index+1);

            new_node.append(keys,values);

            let promoted_key = child.pop_last_key();

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
            let mut size = size_of::<StorageMeta>();

            let mut index = 0;
            while size < self.page_size_half{
                let (key, value) = child.get_key_value_by_index(index);
                size += key.byte_size() as usize;
                size += value.byte_size() as usize; //TODO change return type
                index += 1;
            }

            let mut new_node = LeafNode::new();
            let ( keys, values) = &mut child.split_off(index);
            new_node.append(keys, values);

            let promoted_key = child.get_keys().last().unwrap().clone();

            (promoted_key, new_node)
        };

        let new_page_id = self.page_manager.alloc_node(BTreeNode::Leaf(new_node));

        let parent = self.page_manager.get_node_mut(parent_page)?.as_internal_mut() ;

        parent.insert_key_child(child_idx, promoted_key, child_idx + 1, new_page_id);

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use crate::btree::btree_node::BTreeNode;
    use crate::btree::common::PageId;
    use crate::btree::internal_node::InternalNode;
    use crate::btree::page_manager::PageManager;
    use crate::btree::test_utils::{get_empty_internal_root, get_empty_leaf_root, new_internal, new_leaf};

    #[test]
    fn test_first_set_in_root() {
        let mut tree = get_empty_leaf_root(64);
        tree.set(b"hello", b"world!").unwrap();
        let root = tree.page_manager.get_node(tree.root).unwrap().as_leaf();
        assert_eq!(root.header.keys_total_size, 5);
        assert_eq!(root.header.items_total_size, 6);

        assert_eq!(root.keys, vec![b"hello".to_vec()]);
        assert_eq!(root.values, vec![b"world!".to_vec()]);
    }

    #[test]
    fn leaf_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_leaf_root(64);
        let root = tree.page_manager.get_node(tree.root).unwrap().as_leaf();
        let size_left = root.header.total_size_bytes();

        tree.set(b"hello0", b"world!").unwrap();
        tree.set(b"hello1", b"world!").unwrap();
        tree.set(b"hello2", b"world!").unwrap();

        assert_eq!(tree.page_manager.get_pages().len(), 1);
        //assuming 64-3*13 = 12 bytes left
        tree.maybe_split_root(b"8longkey", b"3val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);
        tree.maybe_split_root(b"8longkey", b"4val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);
        tree.maybe_split_root(b"8longkey", b"05val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);
    }

    #[test]
    fn internal_root_should_decide_to_split_when_full(){
        let mut tree = get_empty_internal_root(64);

        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();

        //48 bytes left
        root.push_lasts(b"hello".to_vec(), 1u64);
        root.push_lasts(b"world".to_vec(), 2u64);
        root.push_child(3u64);

        //14 bytes left
        assert_eq!(tree.page_manager.get_pages().len(), 1);
        tree.maybe_split_root(b"k1", b"4val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);

        tree.maybe_split_root(b"k1", b"000-000-000-000-").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 1);

        tree.maybe_split_root(b"14longlonglong", b"4val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);

        tree.maybe_split_root(b"14longlonglong", b"4val").unwrap();

        //leaves check
        assert!(!tree.page_manager.get_node(0u64).unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(2u64).unwrap().is_leaf());
        assert!(!tree.page_manager.get_node(1u64).unwrap().is_leaf());
    }

    #[test]
    fn test_root_should_split_when_full(){
        let mut tree = get_empty_leaf_root(64);

        tree.set(b"hola0", b"00val").unwrap();
        tree.set(b"hola1", b"01val").unwrap();
        tree.set(b"hola2", b"02val").unwrap();
        tree.set(b"hola3", b"03val").unwrap();

        assert_eq!(tree.page_manager.get_pages().len(), 1);
        assert_eq!(tree.page_manager.get_node(0).unwrap().as_leaf().header.total_size_bytes(), 56);


        tree.set(b"hola4", b"04val").unwrap();
        assert_eq!(tree.page_manager.get_pages().len(), 3);

        let root = tree.page_manager.get_node(tree.root).unwrap().as_internal();
        assert_eq!(root.keys, vec![b"hola1".to_vec()]);
        assert_eq!(root.children.len(), 2);


        let first = tree.page_manager.get_node(root.children[0]).unwrap().as_leaf();
        assert_eq!(first.keys, vec![b"hola0".to_vec(), b"hola1".to_vec()]);
        assert_eq!(first.values, vec![b"00val".to_vec(), b"01val".to_vec()]);


        let second = tree.page_manager.get_node(root.children[1]).unwrap().as_leaf();
        assert_eq!(second.keys, vec![b"hola2".to_vec(), b"hola3".to_vec(), b"hola4".to_vec()]);
        assert_eq!(second.values, vec![b"02val".to_vec(), b"03val".to_vec(), b"04val".to_vec()]);
    }

    #[test]
    fn test_internal_node_split(){
        let mut tree = get_empty_leaf_root(64);

        /*
             root [k6]                   root [k6, k2]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k1]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */

        tree.page_manager = PageManager::new();

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
        let mut tree = get_empty_leaf_root(64);

        /*
             root [k6]                   root [k6, k2]
            /     \                     /  |   \
           [i1]    i2                 i1  i3    i2
       [k0, k1, k2]                 [k0]  [k1]
      [v0,v1,v2,v3]              [v0, v1] [v2,v3]
         split i1
         */

        tree.page_manager = PageManager::new();

        let root_page = new_internal(&mut tree.page_manager);
        tree.root = root_page;

        let i1_page = new_leaf(&mut tree.page_manager);
        let i2_page = new_leaf(&mut tree.page_manager);
        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        root.push_lasts(b"key6".to_vec(), i1_page);
        root.push_child(i2_page);

        let i1 = tree.page_manager.get_node_mut(i1_page).unwrap().as_leaf_mut();

        //48bytes left
        i1.push_lasts(b"key0".to_vec(), b"0-v12345".to_vec());
        i1.push_lasts(b"key1".to_vec(), b"1-v12345".to_vec());
        i1.push_lasts(b"key2".to_vec(), b"2-v12345".to_vec());
        i1.push_lasts(b"key3".to_vec(), b"3-v12345".to_vec());

        tree.split_leaf(tree.root, i1_page, 0).unwrap();

        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        assert_eq!(root.keys, vec![b"key1".to_vec(), b"key6".to_vec()]);
        assert_eq!(root.children.len(), 3);

        let first_child_page = root.get_child_by_index(0);
        let second_child_page = root.get_child_by_index(1);
        let third_child_page = root.get_child_by_index(2);


        assert_eq!(tree.page_manager.get_pages().len()-1, second_child_page as usize);

        let first_child =tree.page_manager.get_node_mut(first_child_page).unwrap().as_leaf_mut();
        assert_eq!(first_child.keys, vec![b"key0".to_vec(), b"key1".to_vec()]);
        assert_eq!(first_child.values, vec![b"0-v12345".to_vec(), b"1-v12345".to_vec()]);

        let second_child =tree.page_manager.get_node_mut(second_child_page).unwrap().as_leaf_mut();
        assert_eq!(second_child.keys, vec![b"key2".to_vec(), b"key3".to_vec()]);
        assert_eq!(second_child.values, vec![b"2-v12345".to_vec(), b"3-v12345".to_vec()]);

        let third_child =tree.page_manager.get_node_mut(third_child_page).unwrap().as_leaf_mut();
        assert_eq!(third_child.keys, Vec::<Vec<u8>>::new());
        assert_eq!(third_child.values, Vec::<Vec<u8>>::new());
    }
}