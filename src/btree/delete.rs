use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::btree::internal_node::InternalNode;
use crate::errors::KvResult;

impl BTree{
    pub fn delete(&mut self, key: &[u8]) -> KvResult<()> {
        let mut curr_page = self.root;
        let mut prev_node_page : Option<PageId> = None;
        let mut prev_child_idx : Option<usize> = None;

        loop {
            let node = self.page_manager.get_node(curr_page)?;

            match node {
                BTreeNode::Leaf(_) => {
                    let node = self.page_manager.get_node_mut(curr_page)?.as_leaf_mut();
                    return node.delete_key(key)
                }
                BTreeNode::Internal(node) => {
                    let child_index = node.route_key_to_index(key);
                    let child_page = node.get_child_by_index(child_index);

                    let child = self.page_manager.get_node(child_page)?;

                    if child.total_size_bytes() > self.page_size_half {
                        prev_node_page = Some(curr_page);
                        prev_child_idx = Some(child_index);
                        curr_page = child_page;
                        continue;
                    }

                    let fallback_donor_index = if child_index == 0 {
                        child_index + 1
                    } else {
                        child_index - 1
                    };

                    let donor_res = self.try_to_find_donor(node, child_index)?;
                    match donor_res {
                        Some((donor_page, donor_is_first)) => {
                            self.fill_up(child_page, child_index, donor_page, donor_is_first, curr_page)?;
                        }
                        None => {
                            let left_idx = fallback_donor_index.min(child_index);
                            let right_idx = fallback_donor_index.max(child_index);
                            let subtree_root = self.merge_children(left_idx, right_idx, curr_page)?;
                            match prev_node_page{
                                Some(prev_node_page) => {
                                    let prev_node = self.page_manager.get_node_mut(prev_node_page)?;
                                    let key_idx = prev_child_idx.unwrap();
                                    prev_node.as_internal_mut().overwrite_value(key_idx, subtree_root);
                                },
                                None => {
                                    self.root = subtree_root;
                                }
                            }
                            curr_page = subtree_root;
                        }
                    }
                }
            }
        }
    }


    fn try_to_find_donor(&self, parent: &InternalNode, child_index: usize) -> KvResult<Option<(PageId, bool)>>{
        if child_index > 0 {
            let prev_page_id = parent.get_child_by_index(child_index-1);
            let prev = self.page_manager.get_node(prev_page_id)?;
            if prev.total_size_bytes() > self.page_size_half && prev.key_count() > 2 {
                return Ok(Some((prev_page_id.clone(), true)))
            }
        }

        let n_children = parent.get_keys().len() + 1;
        if child_index < n_children - 1 {
            let next_page_id = parent.get_child_by_index(child_index+1);
            let next = self.page_manager.get_node(next_page_id)?;
            if next.total_size_bytes() > self.page_size_half && next.key_count() > 2{
                return Ok(Some((next_page_id.clone(), false)))
            }
        }
        Ok(None)
    }


    fn fill_up(&mut self, thin_page: PageId, thin_child_index: usize, donor_page: PageId, donor_is_prev: bool, parent_page: PageId) -> KvResult<()> {


        /*
          [2,5]

        [0,2], [4], [8, 9]

        we fill up index thin = 1 from donor= 0 (donor_is_prev)
              [0,5]

        [0], [2, 4], [8, 9]

        this means that the routing key will idx-1 child.last()
         ------
        if we fill up from the thin=1, donor=2 (!donor_is_prev)
              [0,8]

        [0, 2], [4, 8], [9]
        then the routing key is idx.last()
      */

        let (thin_child, donor_child, parent) = self.page_manager.get_three_mut(
            thin_page, donor_page, parent_page
        )?;
        let parent = parent.as_internal_mut();
        assert_eq!(thin_child.is_leaf(), donor_child.is_leaf());

        match thin_child {
            BTreeNode::Internal(thin_child) => {
                let donor_child = donor_child.as_internal_mut();

                if donor_is_prev {
                    let parent_key_idx = thin_child_index - 1;
                    let parent_key = parent.get_keys()[parent_key_idx].clone();

                    let (donor_key, donor_child_ptr) = donor_child.remove_lasts();

                    // Rotation through parent:
                    // 1. Parent key moves down into thin child
                    // 2. Donor key moves up into parent
                    thin_child.push_firsts(parent_key, donor_child_ptr);
                    parent.overwrite_key(parent_key_idx, donor_key);
                } else {
                    let parent_key_idx = thin_child_index;
                    let parent_key = parent.get_keys()[parent_key_idx].clone();

                    let (donor_key, donor_child_ptr) = donor_child.remove_firsts();

                    // Rotation through parent:
                    // 1. Parent key moves down into thin child
                    // 2. Donor key moves up into parent
                    thin_child.push_lasts(parent_key, donor_child_ptr);
                    parent.overwrite_key(parent_key_idx, donor_key);
                }
                Ok(())
            }
            BTreeNode::Leaf(thin_child) => {
                let donor_child = donor_child.as_leaf_mut();
                if donor_is_prev {
                    let (key, value) = donor_child.remove_lasts();
                    thin_child.push_firsts(key, value);

                    let new_separator = donor_child.get_keys().last().unwrap().clone();
                    parent.overwrite_key(thin_child_index - 1, new_separator);
                } else {
                    let (key, value) = donor_child.remove_firsts();
                    thin_child.push_lasts(key, value);

                    let new_separator = thin_child.get_keys().last().unwrap().clone();
                    parent.overwrite_key(thin_child_index, new_separator);
                }
                Ok(())
            }
        }

    }


    fn merge_children(&mut self, left_child_index: usize, right_child_index: usize, parent_page: PageId) -> KvResult<PageId>{

        assert!(left_child_index < right_child_index);

        let parent = self.page_manager.get_node(parent_page)?.as_internal();
        let pulled_down_key = parent.get_keys()[left_child_index].clone();
        let left_page = parent.get_child_by_index(left_child_index);
        let right_page = parent.get_child_by_index(right_child_index);

        let (left, right, parent) = self.page_manager.get_three_mut(left_page, right_page, parent_page)?;
        let parent = parent.as_internal_mut();
        assert_eq!(left.is_leaf(), right.is_leaf());



        match left {
            //TODO delete nodes/containers? can i move them
            BTreeNode::Internal( left) => {
                let right = right.as_internal_mut();
                let (keys, children) = right.get_key_children_mut();
                keys.insert(0, pulled_down_key);
                left.append(keys, children);
            },
            BTreeNode::Leaf(left) => {
                let right = right.as_leaf_mut();
                let (keys, values) = right.get_key_values_mut();
                left.append(keys, values);
            }
        }
        /*
                        merge(0,1)       merge(1,2)
            [1,2]            [2]           [1]
           [0] [2] [3] -> [0, 2] [3]  or  [0]  [2,3]
                           min(d,t)          min(d,t)

         */
        parent.remove_key_child(left_child_index, left_child_index+1);
        let parent_is_root = self.root == parent_page;
        let subtree_root = if parent_is_root && parent.get_keys().is_empty(){
            self.page_manager.delete(parent_page)?;
            left_page
        }else{
            parent_page
        };
        self.page_manager.delete(right_page)?;

        Ok(subtree_root)
    }
}


#[cfg(test)]
mod tests {
    use crate::btree::test_utils::{get_empty_internal_root, new_internal, new_leaf};


    #[test]
    fn test_internal_node_merge(){
        let mut tree = get_empty_internal_root(64);

        let i1_page = new_internal(&mut tree.page_manager);
        let i2_page = new_internal(&mut tree.page_manager);
        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        root.push_lasts(b"key6".to_vec(), i1_page);
        root.push_child(i2_page);

        let i1 = tree.page_manager.get_node_mut(i1_page).unwrap().as_internal_mut();
        i1.push_lasts(b"key0".to_vec(), 00u64);
        i1.push_child(10u64);

        let i2 = tree.page_manager.get_node_mut(i2_page).unwrap().as_internal_mut();
        i2.push_lasts(b"key7".to_vec(), 70u64);
        i2.push_child(80u64);

        let new_root =  tree.merge_children(0, 1, tree.root).unwrap();
        assert_eq!(new_root, i1_page);
        let root = tree.page_manager.get_node_mut(new_root).unwrap().as_internal_mut();
        assert_eq!(root.keys, vec![b"key0".to_vec(), b"key6".to_vec(), b"key7".to_vec()]);
        assert_eq!(root.children, vec![00u64, 10u64, 70u64, 80u64]);

    }

    #[test]
    fn test_leaf_node_merge(){
        let mut tree = get_empty_internal_root(64);

        let l1_page = new_leaf(&mut tree.page_manager);
        let l2_page = new_leaf(&mut tree.page_manager);
        let root = tree.page_manager.get_node_mut(tree.root).unwrap().as_internal_mut();
        root.push_lasts(b"key6".to_vec(), l1_page);
        root.push_child(l2_page);

        let l1 = tree.page_manager.get_node_mut(l1_page).unwrap().as_leaf_mut();
        l1.push_lasts(b"key0".to_vec(), b"value0".to_vec());

        let l2 = tree.page_manager.get_node_mut(l2_page).unwrap().as_leaf_mut();
        l2.push_lasts(b"key7".to_vec(), b"value7".to_vec());

        let new_root =  tree.merge_children(0, 1, tree.root).unwrap();
        assert_eq!(new_root, l1_page);

        let root = tree.page_manager.get_node_mut(new_root).unwrap().as_leaf_mut();
        assert_eq!(root.keys, vec![b"key0".to_vec(), b"key7".to_vec()]);
        assert_eq!(root.values, vec![b"value0".to_vec(), b"value7".to_vec()]);
    }

}