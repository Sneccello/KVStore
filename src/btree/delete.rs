use std::sync::{RwLockWriteGuard};
use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::btree::page_managers::page_manager::PageManager;
use crate::errors::{KvError, KvResult};

impl BTree{

    pub fn delete(&self, key: &[u8]) -> KvResult<()> {
        let root_guard = self.root.write().map_err(
            |e| KvError::LockError()
        )?;

        let current_arc = self.page_manager.get_node(*root_guard)?;
        let current_guard = current_arc.write().map_err(|e| KvError::LockError())?;

        self.recursive_delete(key, *root_guard, current_guard, true, &mut Some(root_guard) )
    }

    fn recursive_delete<'a>(&self,
                            key: &[u8],
                            current_page: PageId,
                            mut current_guard: RwLockWriteGuard<'a, BTreeNode>,
                            current_is_root: bool,
                            root_guard: &mut Option<RwLockWriteGuard<'_, PageId>>, //TODO?
    ) -> KvResult<()> {
        /*
            1.)lock child
            2.)move to child
            3.)release parent
        */

        match &mut *current_guard {
            BTreeNode::Leaf(leaf_node) => {
                leaf_node.delete_key(key)
            }
            BTreeNode::Internal(internal_node) => {
                let child_index = internal_node.route_key_to_index(key);
                let child_page = internal_node.get_child_by_index(child_index);
                let child_arc = self.page_manager.get_node(child_page)?;
                let mut child_guard = child_arc.write().unwrap();
                if child_guard.total_size_bytes() > self.node_thin_limit_bytes {
                    drop(root_guard.take());
                    drop(current_guard);
                    return self.recursive_delete(key, child_page, child_guard, false, root_guard);
                }

                let (donor_page, donor_is_prev) = if child_index > 0 {
                    (internal_node.get_child_by_index(child_index - 1), true)
                } else {
                    (internal_node.get_child_by_index(child_index + 1), false)
                };

                let donor_arc = self.page_manager.get_node(donor_page)?;
                let mut donor_guard = donor_arc.write().unwrap();

                let can_donate = donor_guard.total_size_bytes() > self.node_thin_limit_bytes && donor_guard.key_count() > 2;

                if can_donate {
                    self.fill_up(&mut child_guard, &mut donor_guard, child_index, donor_is_prev, &mut current_guard)?;
                    drop(donor_guard);
                    drop(current_guard);
                    return self.recursive_delete(key, child_page, child_guard, false, root_guard);
                }else{
                    let (mut left_guard, mut right_guard, left_page, right_page, left_idx) =
                        if donor_is_prev {
                            (donor_guard, child_guard, donor_page, child_page, child_index - 1)
                        } else {
                            (child_guard, donor_guard, child_page, donor_page, child_index)
                        };


                    self.merge_children(&mut left_guard, &mut right_guard, left_idx, &mut current_guard);

                    self.page_manager.delete(right_page)?;
                    drop(right_guard);
                    let current = current_guard.as_internal_mut();

                    current.remove_key_child(left_idx, left_idx+1);

                    let mut next_is_root = false;

                    if current_is_root && current.get_keys().is_empty(){
                        //the current node has at least 2 keys. if its empty then it must be root
                        drop(current_guard);
                        self.page_manager.delete(current_page)?;

                        if let Some(root_guard) = root_guard.as_mut() {
                            **root_guard = left_page;
                        }
                        next_is_root = true;
                    }else{
                        drop(current_guard);
                    }

                    //TODO make sure left page is valid
                    return self.recursive_delete(key, left_page, left_guard, next_is_root, root_guard);



                }
            }
        }

    }




    fn fill_up(&self, thin_child: &mut RwLockWriteGuard<BTreeNode>, donor_child: &mut RwLockWriteGuard<BTreeNode>,
               thin_child_index: usize, donor_is_prev: bool, parent: &mut RwLockWriteGuard<BTreeNode>) -> KvResult<()> {


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


        let parent = parent.as_internal_mut();
        assert_eq!(thin_child.is_leaf(), donor_child.is_leaf());

        match &mut **thin_child {
            BTreeNode::Internal(thin_child) => {
                let mut donor_child = donor_child.as_internal_mut();

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


    fn merge_children(&self, left_child: &mut RwLockWriteGuard<BTreeNode>, right_child: &mut RwLockWriteGuard<BTreeNode>,
                      left_child_index: usize, parent_guard: &mut RwLockWriteGuard<BTreeNode>){
        /*
            merge(0,1)       merge(1,2)
                [1,2]            [2]           [1]
                [0] [2] [3] -> [0, 2] [3]  or  [0]  [2,3]
                               min(d,t)          min(d,t)

        */
        let parent = parent_guard.as_internal_mut();
        let pulled_down_key = parent.get_keys()[left_child_index].clone();


        match &mut **left_child {
            //TODO delete nodes/containers? can i move them
            BTreeNode::Internal(left) => {
                let (keys, children) = right_child.as_internal_mut().get_key_children_mut();
                keys.insert(0, pulled_down_key);
                left.append(keys, children);
            },
            BTreeNode::Leaf(left) => {
                let (keys, values) = right_child.as_leaf_mut().get_key_values_mut();
                left.append(keys, values);
            }
        }

    }
}


#[cfg(test)]
mod tests {
    use crate::btree::test_utils::{get_empty_internal_root, get_root_page, new_internal, new_leaf};


    #[test]
    fn test_internal_node_merge(){
        let mut tree = get_empty_internal_root(64);

        let i1_page = new_internal(&mut tree.page_manager);
        let i2_page = new_internal(&mut tree.page_manager);
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root_node = root_guard.as_internal_mut();
            root_node.push_lasts(b"key6".to_vec(), i1_page);
            root_node.push_child(i2_page);

            let i1_lock = tree.page_manager.get_node(i1_page).unwrap();
            let mut i1_guard = i1_lock.write().unwrap();
            let i1 = i1_guard.as_internal_mut();
            i1.push_lasts(b"key0".to_vec(), 00u64);
            i1.push_child(10u64);

            let i2_lock = tree.page_manager.get_node(i2_page).unwrap();
            let mut i2_guard = i2_lock.write().unwrap();
            let i2 = i2_guard.as_internal_mut();
            i2.push_lasts(b"key7".to_vec(), 70u64);
            i2.push_child(80u64);

            tree.merge_children(&mut i1_guard,&mut i2_guard,0,  &mut root_guard);
        };
        {
            let root_lock = tree.page_manager.get_node(i1_page).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root_node = root_guard.as_internal_mut();
            assert_eq!(root_node.keys, vec![b"key0".to_vec(), b"key6".to_vec(), b"key7".to_vec()]);
            assert_eq!(root_node.children, vec![00u64, 10u64, 70u64, 80u64]);
        }
    }

    #[test]
    fn test_leaf_node_merge(){
        let mut tree = get_empty_internal_root(64);

        let l1_page = new_leaf(&mut tree.page_manager);
        let l2_page = new_leaf(&mut tree.page_manager);
        {
            let root_lock = tree.page_manager.get_node(get_root_page(&tree)).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let root_node = root_guard.as_internal_mut();
            root_node.push_lasts(b"key6".to_vec(), l1_page);
            root_node.push_child(l2_page);

            let l1_lock = tree.page_manager.get_node(l1_page).unwrap();
            let mut l1_guard = l1_lock.write().unwrap();
            let l1 = l1_guard.as_leaf_mut();
            l1.push_lasts(b"key0".to_vec(), b"value0".to_vec());

            let l2_lock = tree.page_manager.get_node(l2_page).unwrap();
            let mut l2_guard = l2_lock.write().unwrap();
            let l2 = l2_guard.as_leaf_mut();
            l2.push_lasts(b"key7".to_vec(), b"value7".to_vec());

            drop(root_guard);
            let parent_page = get_root_page(&tree);
            let root_lock = tree.page_manager.get_node(parent_page).unwrap();
            let mut root_guard = root_lock.write().unwrap();
            let new_root =  tree.merge_children(&mut l1_guard,&mut l2_guard, 0, &mut root_guard);

        };
        let root_lock = tree.page_manager.get_node(l1_page).unwrap();
        let mut root_guard = root_lock.write().unwrap();
        let root_node = root_guard.as_leaf_mut();
        assert_eq!(root_node.keys, vec![b"key0".to_vec(), b"key7".to_vec()]);
        assert_eq!(root_node.values, vec![b"value0".to_vec(), b"value7".to_vec()]);
    }
}

//TODO test scale back to leaf?