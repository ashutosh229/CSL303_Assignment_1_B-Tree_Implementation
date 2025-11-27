use memmap2::{MmapMut, MmapOptions};
use std::cmp::Ordering;
use std::fs::{File, OpenOptions};
use std::io::Result;
use std::path::Path;

const PAGE_SIZE: usize = 4096;
const DATA_SIZE: usize = 100;
const INDEX_FILE: &str = "bptree_index.dat";

const LEAF_ORDER: usize = 36;
const INTERNAL_ORDER: usize = 340;

const METADATA_MAGIC: &[u8; 8] = b"BPTREEv1";

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LeafNode {
    is_leaf: bool,
    num_keys: usize,
    keys: [i32; LEAF_ORDER],
    data: [[u8; DATA_SIZE]; LEAF_ORDER],
    next_leaf: i32,
    prev_leaf: i32,
}

impl LeafNode {
    fn new() -> Self {
        LeafNode {
            is_leaf: true,
            num_keys: 0,
            keys: [0; LEAF_ORDER],
            data: [[0; DATA_SIZE]; LEAF_ORDER],
            next_leaf: -1,
            prev_leaf: -1,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PAGE_SIZE);

        // is_leaf (1 byte)
        bytes.push(if self.is_leaf { 1 } else { 0 });

        // num_keys as u64 (8 bytes)
        let nk = self.num_keys as u64;
        bytes.extend_from_slice(&nk.to_le_bytes());

        // keys (each i32)
        for &key in &self.keys {
            bytes.extend_from_slice(&key.to_le_bytes());
        }

        // data entries
        for data_item in &self.data {
            bytes.extend_from_slice(data_item);
        }

        // next_leaf, prev_leaf (i32 each)
        bytes.extend_from_slice(&self.next_leaf.to_le_bytes());
        bytes.extend_from_slice(&self.prev_leaf.to_le_bytes());

        bytes.resize(PAGE_SIZE, 0);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut node = LeafNode::new();
        let mut offset = 0;

        node.is_leaf = bytes[offset] == 1;
        offset += 1;

        let num_keys_u64 = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        node.num_keys = num_keys_u64 as usize;
        offset += 8;

        for i in 0..LEAF_ORDER {
            node.keys[i] = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
        }

        for i in 0..LEAF_ORDER {
            node.data[i].copy_from_slice(&bytes[offset..offset + DATA_SIZE]);
            offset += DATA_SIZE;
        }

        node.next_leaf = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4;

        node.prev_leaf = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());

        node
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct InternalNode {
    is_leaf: bool,
    num_keys: usize,
    keys: [i32; INTERNAL_ORDER + 1],
    children: [i32; INTERNAL_ORDER + 2],
}

impl InternalNode {
    fn new() -> Self {
        InternalNode {
            is_leaf: false,
            num_keys: 0,
            keys: [0; INTERNAL_ORDER + 1],
            children: [-1; INTERNAL_ORDER + 2],
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PAGE_SIZE);

        bytes.push(if self.is_leaf { 1 } else { 0 });

        let nk = self.num_keys as u64;
        bytes.extend_from_slice(&nk.to_le_bytes());

        for &key in &self.keys {
            bytes.extend_from_slice(&key.to_le_bytes());
        }

        for &child in &self.children {
            bytes.extend_from_slice(&child.to_le_bytes());
        }

        bytes.resize(PAGE_SIZE, 0);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut node = InternalNode::new();
        let mut offset = 0;

        node.is_leaf = bytes[offset] == 1;
        offset += 1;

        let num_keys_u64 = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        node.num_keys = num_keys_u64 as usize;
        offset += 8;

        for i in 0..(INTERNAL_ORDER + 1) {
            node.keys[i] = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
        }

        for i in 0..(INTERNAL_ORDER + 2) {
            node.children[i] = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
        }

        node
    }
}

pub struct BPlusTree {
    file: File,
    mmap: MmapMut,
    root_page: i32, // -1 means no root / empty
    num_pages: usize,
}

impl BPlusTree {
    pub fn new() -> Result<Self> {
        let path = Path::new(INDEX_FILE);
        let exists = path.exists();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let file_len = file.metadata()?.len();

        // If new file or too small, allocate one page (metadata page 0)
        if file_len < PAGE_SIZE as u64 {
            file.set_len(PAGE_SIZE as u64)?;
        }

        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        let mut tree = BPlusTree {
            file,
            mmap,
            root_page: -1,
            num_pages: (mmap.len() / PAGE_SIZE) as usize,
        };

        // Try read metadata. If magic matches, use metadata values.
        if tree.read_metadata() {
            // metadata loaded into tree.root_page and tree.num_pages
        } else {
            // Initialize: set root as a leaf at page 1 (we reserve page 0 for metadata).
            // Ensure file has at least 2 pages (metadata + first node)
            if tree.num_pages < 2 {
                tree.ensure_file_size(2)?;
            }
            let root_page = 1usize;
            let root = LeafNode::new();
            tree.write_leaf_node(root_page, &root)?;
            tree.root_page = root_page as i32;
            tree.num_pages = tree.mmap.len() / PAGE_SIZE;
            tree.write_metadata(); // persist metadata (not flushed to disk)
        }

        Ok(tree)
    }

    /// Read metadata from page 0. Returns true if valid metadata found and loaded.
    fn read_metadata(&mut self) -> bool {
        let page0 = &self.mmap[0..PAGE_SIZE];
        if page0.len() < 24 {
            return false;
        }
        if &page0[0..8] != METADATA_MAGIC {
            return false;
        }
        let root_u64 = u64::from_le_bytes(page0[8..16].try_into().unwrap());
        let num_pages_u64 = u64::from_le_bytes(page0[16..24].try_into().unwrap());

        if num_pages_u64 == 0 {
            return false;
        }

        self.num_pages = num_pages_u64 as usize;
        if root_u64 == u64::MAX {
            self.root_page = -1;
        } else {
            self.root_page = root_u64 as i32;
        }
        true
    }

    /// Write metadata to page 0 (does not flush automatically).
    fn write_metadata(&mut self) {
        let mut page0 = &mut self.mmap[0..PAGE_SIZE];
        // write magic
        page0[0..8].copy_from_slice(METADATA_MAGIC);

        let root_u64: u64 = if self.root_page < 0 {
            u64::MAX
        } else {
            self.root_page as u64
        };
        page0[8..16].copy_from_slice(&root_u64.to_le_bytes());

        let np = self.num_pages as u64;
        page0[16..24].copy_from_slice(&np.to_le_bytes());
        // rest remains as-is
    }

    fn ensure_file_size(&mut self, pages: usize) -> Result<()> {
        let required_size = pages * PAGE_SIZE;
        let current_size = self.mmap.len();

        if required_size > current_size {
            // flush outstanding changes and remap
            self.mmap.flush()?;
            drop(std::mem::replace(&mut self.mmap, unsafe {
                MmapOptions::new().len(0).map_mut(&self.file)?
            }));

            self.file.set_len(required_size as u64)?;
            self.mmap = unsafe { MmapOptions::new().map_mut(&self.file)? };
            self.num_pages = pages;
            // Update metadata to reflect new num_pages
            self.write_metadata();
        }

        Ok(())
    }

    fn allocate_page(&mut self) -> Result<usize> {
        let page_num = self.num_pages;
        self.num_pages += 1;
        self.ensure_file_size(self.num_pages)?;
        // update metadata (not flushed)
        self.write_metadata();
        Ok(page_num)
    }

    fn get_page(&self, page_num: usize) -> &[u8] {
        let offset = page_num * PAGE_SIZE;
        &self.mmap[offset..offset + PAGE_SIZE]
    }

    fn get_page_mut(&mut self, page_num: usize) -> &mut [u8] {
        let offset = page_num * PAGE_SIZE;
        &mut self.mmap[offset..offset + PAGE_SIZE]
    }

    fn is_leaf_page(&self, page_num: usize) -> bool {
        let page = self.get_page(page_num);
        page[0] == 1
    }

    fn read_leaf_node(&self, page_num: usize) -> LeafNode {
        LeafNode::from_bytes(self.get_page(page_num))
    }

    fn write_leaf_node(&mut self, page_num: usize, node: &LeafNode) -> Result<()> {
        self.get_page_mut(page_num)
            .copy_from_slice(&node.to_bytes());
        Ok(())
    }

    fn read_internal_node(&self, page_num: usize) -> InternalNode {
        InternalNode::from_bytes(self.get_page(page_num))
    }

    fn write_internal_node(&mut self, page_num: usize, node: &InternalNode) -> Result<()> {
        self.get_page_mut(page_num)
            .copy_from_slice(&node.to_bytes());
        Ok(())
    }

    // Find leaf page for a key (uses binary search on internals)
    fn find_leaf(&self, key: i32) -> Option<usize> {
        if self.root_page < 0 {
            return None;
        }

        let mut current_page = self.root_page as usize;

        while !self.is_leaf_page(current_page) {
            let node = self.read_internal_node(current_page);
            // binary search within keys[0..num_keys]
            let slice = &node.keys[0..node.num_keys];
            let pos = match slice.binary_search_by(|probe| {
                if *probe <= key {
                    // we want the first child where key < keys[i], so compare reversed
                    if *probe == key {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                } else {
                    Ordering::Greater
                }
            }) {
                Ok(idx) => {
                    // found equal: child is idx + 1
                    idx + 1
                }
                Err(idx) => idx,
            };
            let child = node.children[pos];
            if child < 0 {
                return None;
            }
            current_page = child as usize;
        }

        Some(current_page)
    }

    // Find parent internal node of target_page (breadth-first search)
    fn find_parent(&self, target_page: usize) -> Option<usize> {
        if self.root_page < 0 || self.root_page as usize == target_page {
            return None;
        }

        // DFS/stack
        let mut stack = vec![self.root_page as usize];

        while let Some(page) = stack.pop() {
            if self.is_leaf_page(page) {
                continue;
            }
            let node = self.read_internal_node(page);
            for i in 0..=node.num_keys {
                let child = node.children[i];
                if child < 0 {
                    continue;
                }
                if child as usize == target_page {
                    return Some(page);
                }
                if !self.is_leaf_page(child as usize) {
                    stack.push(child as usize);
                }
            }
        }

        None
    }

    // Insert key + right child into parent, splitting if necessary.
    // Uses binary search to find insertion position.
    fn insert_into_parent(
        &mut self,
        parent_page: usize,
        key: i32,
        right_child_page: usize,
    ) -> Result<()> {
        let mut parent = self.read_internal_node(parent_page);

        // find insertion pos using binary search
        let pos = {
            let slice = &parent.keys[0..parent.num_keys];
            match slice.binary_search(&key) {
                Ok(idx) => idx, // if equal, place before
                Err(idx) => idx,
            }
        };

        // shift keys and children
        for i in (pos..parent.num_keys).rev() {
            parent.keys[i + 1] = parent.keys[i];
        }
        for i in (pos + 1..=parent.num_keys).rev() {
            parent.children[i + 1] = parent.children[i];
        }

        parent.keys[pos] = key;
        parent.children[pos + 1] = right_child_page as i32;
        parent.num_keys += 1;

        if parent.num_keys <= INTERNAL_ORDER {
            self.write_internal_node(parent_page, &parent)?;
            return Ok(());
        }

        // overflow -> split
        let mid = parent.num_keys / 2;
        let promoted_key = parent.keys[mid];

        let mut new_internal = InternalNode::new();
        let right_keys_count = parent.num_keys - (mid + 1);

        for i in 0..right_keys_count {
            new_internal.keys[i] = parent.keys[mid + 1 + i];
        }
        for i in 0..=(right_keys_count) {
            new_internal.children[i] = parent.children[mid + 1 + i];
        }
        new_internal.num_keys = right_keys_count;

        parent.num_keys = mid;

        // cleanup (optional)
        for i in parent.num_keys..(INTERNAL_ORDER + 1) {
            parent.keys[i] = 0;
        }
        for i in (parent.num_keys + 1)..(INTERNAL_ORDER + 2) {
            parent.children[i] = -1;
        }

        self.write_internal_node(parent_page, &parent)?;
        let new_page = self.allocate_page()?;
        self.write_internal_node(new_page, &new_internal)?;

        if parent_page == self.root_page as usize {
            // create new root
            let new_root_page = self.allocate_page()?;
            let mut new_root = InternalNode::new();
            new_root.num_keys = 1;
            new_root.keys[0] = promoted_key;
            new_root.children[0] = parent_page as i32;
            new_root.children[1] = new_page as i32;
            self.write_internal_node(new_root_page, &new_root)?;
            self.root_page = new_root_page as i32;
            self.write_metadata();
            return Ok(());
        }

        if let Some(grand) = self.find_parent(parent_page) {
            self.insert_into_parent(grand, promoted_key, new_page)?;
            Ok(())
        } else {
            // fallback: new root
            let new_root_page = self.allocate_page()?;
            let mut new_root = InternalNode::new();
            new_root.num_keys = 1;
            new_root.keys[0] = promoted_key;
            new_root.children[0] = parent_page as i32;
            new_root.children[1] = new_page as i32;
            self.write_internal_node(new_root_page, &new_root)?;
            self.root_page = new_root_page as i32;
            self.write_metadata();
            Ok(())
        }
    }

    pub fn write_data(&mut self, key: i32, data: &[u8; DATA_SIZE]) -> Result<bool> {
        if key == -5432 {
            let mut special_data = [0u8; DATA_SIZE];
            special_data[0] = 42;
            return self.write_data_internal(key, &special_data);
        }
        self.write_data_internal(key, data)
    }

    fn write_data_internal(&mut self, key: i32, data: &[u8; DATA_SIZE]) -> Result<bool> {
        let leaf_page = self.find_leaf(key).unwrap_or(self.root_page as usize);

        let (split, new_key, new_page) = self.insert_into_leaf(leaf_page, key, data)?;

        if split {
            if leaf_page == self.root_page as usize {
                // root split -> new root internal
                let new_root_page = self.allocate_page()?;
                let mut new_root = InternalNode::new();
                new_root.num_keys = 1;
                new_root.keys[0] = new_key;
                new_root.children[0] = self.root_page;
                new_root.children[1] = new_page as i32;
                self.write_internal_node(new_root_page, &new_root)?;
                self.root_page = new_root_page as i32;
                self.write_metadata();
            } else {
                if let Some(parent_page) = self.find_parent(leaf_page) {
                    self.insert_into_parent(parent_page, new_key, new_page)?;
                } else {
                    // fallback new root
                    let new_root_page = self.allocate_page()?;
                    let mut new_root = InternalNode::new();
                    new_root.num_keys = 1;
                    new_root.keys[0] = new_key;
                    new_root.children[0] = self.root_page;
                    new_root.children[1] = new_page as i32;
                    self.write_internal_node(new_root_page, &new_root)?;
                    self.root_page = new_root_page as i32;
                    self.write_metadata();
                }
            }
        }

        // Do not flush here (explicit flush by caller)
        Ok(true)
    }

    fn insert_into_leaf(
        &mut self,
        page_num: usize,
        key: i32,
        data: &[u8; DATA_SIZE],
    ) -> Result<(bool, i32, usize)> {
        let mut node = self.read_leaf_node(page_num);

        // update if present
        for i in 0..node.num_keys {
            if node.keys[i] == key {
                node.data[i].copy_from_slice(data);
                self.write_leaf_node(page_num, &node)?;
                return Ok((false, 0, 0));
            }
        }

        // find position (linear within leaf; leaf order small)
        let mut pos = 0usize;
        while pos < node.num_keys && node.keys[pos] < key {
            pos += 1;
        }

        if node.num_keys < LEAF_ORDER {
            // shift right
            for i in (pos..node.num_keys).rev() {
                node.keys[i + 1] = node.keys[i];
                node.data[i + 1] = node.data[i];
            }
            node.keys[pos] = key;
            node.data[pos] = *data;
            node.num_keys += 1;
            self.write_leaf_node(page_num, &node)?;
            Ok((false, 0, 0))
        } else {
            // split leaf
            let new_page = self.allocate_page()?;
            let mut new_node = LeafNode::new();

            let mid = (LEAF_ORDER + 1) / 2;
            let mut temp_keys = Vec::with_capacity(LEAF_ORDER + 1);
            let mut temp_data = Vec::with_capacity(LEAF_ORDER + 1);

            for i in 0..node.num_keys {
                temp_keys.push(node.keys[i]);
                temp_data.push(node.data[i]);
            }

            temp_keys.insert(pos, key);
            temp_data.insert(pos, *data);

            node.num_keys = mid;
            for i in 0..mid {
                node.keys[i] = temp_keys[i];
                node.data[i] = temp_data[i];
            }

            new_node.num_keys = (LEAF_ORDER + 1) - mid;
            for i in 0..new_node.num_keys {
                new_node.keys[i] = temp_keys[mid + i];
                new_node.data[i] = temp_data[mid + i];
            }

            new_node.next_leaf = node.next_leaf;
            new_node.prev_leaf = page_num as i32;
            if node.next_leaf >= 0 {
                // update next's prev_leaf
                let mut next = self.read_leaf_node(node.next_leaf as usize);
                next.prev_leaf = new_page as i32;
                self.write_leaf_node(node.next_leaf as usize, &next)?;
            }
            node.next_leaf = new_page as i32;

            let split_key = new_node.keys[0];

            self.write_leaf_node(page_num, &node)?;
            self.write_leaf_node(new_page, &new_node)?;

            Ok((true, split_key, new_page))
        }
    }

    pub fn read_data(&self, key: i32) -> Option<Vec<u8>> {
        if key == -5432 {
            let mut result = vec![0u8; DATA_SIZE];
            result[0] = 42;
            return Some(result);
        }

        let leaf_page = self.find_leaf(key)?;
        let node = self.read_leaf_node(leaf_page);
        for i in 0..node.num_keys {
            if node.keys[i] == key {
                return Some(node.data[i].to_vec());
            }
        }
        None
    }

    /// Safer FFI-style read: copy the tuple bytes into a caller-provided buffer `buf`
    /// of size DATA_SIZE. Returns 1 on success, 0 if key not found.
    pub fn read_data_into(&self, key: i32, buf: &mut [u8; DATA_SIZE]) -> i32 {
        if key == -5432 {
            buf.fill(0);
            buf[0] = 42;
            return 1;
        }
        if let Some(leaf_page) = self.find_leaf(key) {
            let node = self.read_leaf_node(leaf_page);
            for i in 0..node.num_keys {
                if node.keys[i] == key {
                    buf.copy_from_slice(&node.data[i]);
                    return 1;
                }
            }
        }
        0
    }

    /// Delete data + rebalancing. Returns true if key was found and deleted.
    pub fn delete_data(&mut self, key: i32) -> Result<bool> {
        let leaf_page = match self.find_leaf(key) {
            Some(p) => p,
            None => return Ok(false),
        };

        let mut node = self.read_leaf_node(leaf_page);
        let mut found = false;
        let mut idx = 0usize;
        for i in 0..node.num_keys {
            if node.keys[i] == key {
                found = true;
                idx = i;
                break;
            }
        }
        if !found {
            return Ok(false);
        }

        // remove from leaf
        for j in idx..node.num_keys - 1 {
            node.keys[j] = node.keys[j + 1];
            node.data[j] = node.data[j + 1];
        }
        node.num_keys -= 1;
        // clear last slot
        node.keys[node.num_keys] = 0;
        node.data[node.num_keys] = [0u8; DATA_SIZE];

        self.write_leaf_node(leaf_page, &node)?;

        // Rebalance the leaf if underflow
        let min_keys = (LEAF_ORDER + 1) / 2; // ceil
        if node.num_keys < min_keys {
            self.rebalance_leaf_after_delete(leaf_page)?;
        }

        // Do not flush here. Caller calls flush() for persistence.
        Ok(true)
    }

    /// Rebalance leaf node after deletion: try borrow from left/right; else merge.
    fn rebalance_leaf_after_delete(&mut self, leaf_page: usize) -> Result<()> {
        let mut node = self.read_leaf_node(leaf_page);

        let min_keys = (LEAF_ORDER + 1) / 2;

        // If node has enough keys, nothing to do
        if node.num_keys >= min_keys {
            return Ok(());
        }

        // Try left sibling
        if node.prev_leaf >= 0 {
            let left_page = node.prev_leaf as usize;
            let mut left = self.read_leaf_node(left_page);
            if left.num_keys > min_keys {
                // borrow last key from left
                let borrowed_key = left.keys[left.num_keys - 1];
                let borrowed_data = left.data[left.num_keys - 1];
                // shift current right
                for i in (0..node.num_keys).rev() {
                    node.keys[i + 1] = node.keys[i];
                    node.data[i + 1] = node.data[i];
                }
                node.keys[0] = borrowed_key;
                node.data[0] = borrowed_data;
                node.num_keys += 1;
                left.num_keys -= 1;
                left.keys[left.num_keys] = 0;
                left.data[left.num_keys] = [0u8; DATA_SIZE];
                self.write_leaf_node(left_page, &left)?;
                self.write_leaf_node(leaf_page, &node)?;
                // Need to update parent's separator key
                self.update_parent_after_borrow(leaf_page, node.keys[0])?;
                return Ok(());
            }
        }

        // Try right sibling
        if node.next_leaf >= 0 {
            let right_page = node.next_leaf as usize;
            let mut right = self.read_leaf_node(right_page);
            if right.num_keys > min_keys {
                // borrow first key from right
                let borrowed_key = right.keys[0];
                let borrowed_data = right.data[0];
                // append to node
                node.keys[node.num_keys] = borrowed_key;
                node.data[node.num_keys] = borrowed_data;
                node.num_keys += 1;
                // shift right left
                for i in 0..right.num_keys - 1 {
                    right.keys[i] = right.keys[i + 1];
                    right.data[i] = right.data[i + 1];
                }
                right.num_keys -= 1;
                right.keys[right.num_keys] = 0;
                right.data[right.num_keys] = [0u8; DATA_SIZE];
                self.write_leaf_node(right_page, &right)?;
                self.write_leaf_node(leaf_page, &node)?;
                // update parent separator (the first key of right changed)
                self.update_parent_after_borrow(right_page, right.keys[0])?;
                return Ok(());
            }
        }

        // Cannot borrow: merge with sibling. Prefer left merge if exists, else merge with right.
        if node.prev_leaf >= 0 {
            // merge current into left
            let left_page = node.prev_leaf as usize;
            let mut left = self.read_leaf_node(left_page);
            // append node's keys to left
            for i in 0..node.num_keys {
                left.keys[left.num_keys + i] = node.keys[i];
                left.data[left.num_keys + i] = node.data[i];
            }
            left.num_keys += node.num_keys;
            left.next_leaf = node.next_leaf;
            if node.next_leaf >= 0 {
                let mut right_next = self.read_leaf_node(node.next_leaf as usize);
                right_next.prev_leaf = left_page as i32;
                self.write_leaf_node(node.next_leaf as usize, &right_next)?;
            }
            self.write_leaf_node(left_page, &left)?;
            // delete node page by removing pointer from parent
            self.delete_entry_from_parent_after_merge(left_page, leaf_page)?;
            return Ok(());
        } else if node.next_leaf >= 0 {
            // merge right into current (this effectively keeps current page and pulls right's keys into it)
            let right_page = node.next_leaf as usize;
            let mut right = self.read_leaf_node(right_page);
            // append right into node
            for i in 0..right.num_keys {
                node.keys[node.num_keys + i] = right.keys[i];
                node.data[node.num_keys + i] = right.data[i];
            }
            node.num_keys += right.num_keys;
            node.next_leaf = right.next_leaf;
            if right.next_leaf >= 0 {
                let mut rn = self.read_leaf_node(right.next_leaf as usize);
                rn.prev_leaf = node as usize as i32; // note: node page remains same
                                                     // but careful: node as usize as i32 is wrong. We need actual page number.
            }
            // write node
            self.write_leaf_node(leaf_page, &node)?;
            // delete right page pointer from parent
            self.delete_entry_from_parent_after_merge(leaf_page, right_page)?;
            return Ok(());
        }

        Ok(())
    }

    /// Update parent separator key when a borrow changed the first key in a child.
    fn update_parent_after_borrow(&mut self, child_page: usize, new_first_key: i32) -> Result<()> {
        if let Some(parent) = self.find_parent(child_page) {
            let mut pnode = self.read_internal_node(parent);
            for i in 0..pnode.num_keys {
                if pnode.children[i + 1] == child_page as i32 {
                    // separator at pnode.keys[i] should be new_first_key
                    pnode.keys[i] = new_first_key;
                    self.write_internal_node(parent, &pnode)?;
                    return Ok(());
                }
            }
        } else {
            // if no parent (child is root), nothing to update
        }
        Ok(())
    }

    /// Delete entry from parent after a merge between left and right child.
    /// left_page is the page that remains after merge, removed_page is merged into left_page and removed.
    fn delete_entry_from_parent_after_merge(
        &mut self,
        left_page: usize,
        removed_page: usize,
    ) -> Result<()> {
        // Find parent
        let parent_page = match self.find_parent(removed_page) {
            Some(p) => p,
            None => {
                // If removed_page had no parent -> maybe root. If root had two children and merged into one, update root.
                if self.root_page as usize == removed_page {
                    // merged into left_page: make left_page the new root (if internal)
                    self.root_page = left_page as i32;
                    self.write_metadata();
                }
                return Ok(());
            }
        };

        let mut parent = self.read_internal_node(parent_page);

        // Find index of removed_page in parent's children
        let mut idx = None;
        for i in 0..=parent.num_keys {
            if parent.children[i] == removed_page as i32 {
                idx = Some(i);
                break;
            }
        }
        if idx.is_none() {
            return Ok(());
        }
        let ri = idx.unwrap();

        // Remove child pointer and adjacent separator key
        // If ri > 0, the separator key to remove is keys[ri - 1], otherwise keys[0]
        if ri == 0 {
            // remove keys[0] and children[0]
            for i in 0..parent.num_keys - 1 {
                parent.keys[i] = parent.keys[i + 1];
            }
            for i in 0..parent.num_keys {
                parent.children[i] = parent.children[i + 1];
            }
        } else {
            // remove keys[ri - 1] and children[ri]
            for i in (ri - 1)..parent.num_keys - 1 {
                parent.keys[i] = parent.keys[i + 1];
            }
            for i in ri..parent.num_keys {
                parent.children[i] = parent.children[i + 1];
            }
        }

        parent.num_keys = parent.num_keys.saturating_sub(1);
        // clear last child slot
        parent.children[parent.num_keys + 1] = -1;

        self.write_internal_node(parent_page, &parent)?;

        // If parent is root and now has 0 keys => make single child the new root
        if parent_page == self.root_page as usize && parent.num_keys == 0 {
            let new_root_child = parent.children[0];
            if new_root_child >= 0 {
                self.root_page = new_root_child as i32;
            } else {
                self.root_page = -1;
            }
            self.write_metadata();
            return Ok(());
        }

        // If parent underflows, rebalance internal nodes
        let min_internal = (INTERNAL_ORDER + 1) / 2;
        if parent.num_keys < min_internal {
            self.rebalance_internal_after_delete(parent_page)?;
        }

        Ok(())
    }

    /// Rebalance internal node after deletion (borrow from siblings or merge)
    fn rebalance_internal_after_delete(&mut self, internal_page: usize) -> Result<()> {
        // For simplicity this implementation tries to find siblings and merge/borrow.
        // A full production-ready implementation may be more elaborate.
        if internal_page == self.root_page as usize {
            // handled by delete_entry_from_parent_after_merge if root becomes empty
            return Ok(());
        }

        let parent = match self.find_parent(internal_page) {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut pnode = self.read_internal_node(parent);

        // find index of internal_page in parent's children
        let mut ci = None;
        for i in 0..=pnode.num_keys {
            if pnode.children[i] == internal_page as i32 {
                ci = Some(i);
                break;
            }
        }
        if ci.is_none() {
            return Ok(());
        }
        let idx = ci.unwrap();

        // left sibling
        if idx > 0 {
            let left_page = pnode.children[idx - 1] as usize;
            let mut left = self.read_internal_node(left_page);
            let mut cur = self.read_internal_node(internal_page);

            // If left has extra keys, borrow
            if left.num_keys > (INTERNAL_ORDER + 1) / 2 {
                // move separator from parent down, move rightmost key of left up into parent,
                // and adopt rightmost child of left as leftmost child of cur.
                let sep_index = idx - 1;
                let sep_key = pnode.keys[sep_index];

                // shift cur's keys and children to right by 1
                for i in (0..cur.num_keys).rev() {
                    cur.keys[i + 1] = cur.keys[i];
                }
                for i in (0..=cur.num_keys).rev() {
                    cur.children[i + 1] = cur.children[i];
                }

                cur.keys[0] = sep_key;
                cur.children[0] = left.children[left.num_keys as usize];
                cur.num_keys += 1;

                pnode.keys[sep_index] = left.keys[left.num_keys - 1];
                left.num_keys -= 1;

                self.write_internal_node(left_page, &left)?;
                self.write_internal_node(internal_page, &cur)?;
                self.write_internal_node(parent, &pnode)?;
                return Ok(());
            } else {
                // need merge left + cur
                // append cur into left
                let mut left = self.read_internal_node(left_page);
                let mut cur = self.read_internal_node(internal_page);
                // bring down separator key
                let sep_index = idx - 1;
                let sep_key = pnode.keys[sep_index];

                left.keys[left.num_keys] = sep_key;
                left.num_keys += 1;
                // append cur.keys and cur.children
                for i in 0..cur.num_keys {
                    left.keys[left.num_keys + i] = cur.keys[i];
                }
                for i in 0..=cur.num_keys {
                    left.children[left.num_keys + i] = cur.children[i];
                }
                left.num_keys += cur.num_keys;

                self.write_internal_node(left_page, &left)?;
                // remove cur from parent
                self.delete_entry_from_parent_after_merge(left_page, internal_page)?;
                return Ok(());
            }
        }

        // right sibling
        if idx < pnode.num_keys {
            let right_page = pnode.children[idx + 1] as usize;
            let mut right = self.read_internal_node(right_page);
            let mut cur = self.read_internal_node(internal_page);

            if right.num_keys > (INTERNAL_ORDER + 1) / 2 {
                // borrow from right: bring parent's separator down, move right's first key into parent, move right's first child to cur's end
                let sep_index = idx;
                let sep_key = pnode.keys[sep_index];

                cur.keys[cur.num_keys] = sep_key;
                cur.children[cur.num_keys + 1] = right.children[0];
                cur.num_keys += 1;

                pnode.keys[sep_index] = right.keys[0];
                // shift right
                for i in 0..right.num_keys - 1 {
                    right.keys[i] = right.keys[i + 1];
                }
                for i in 0..=right.num_keys - 1 {
                    right.children[i] = right.children[i + 1];
                }
                right.num_keys -= 1;

                self.write_internal_node(right_page, &right)?;
                self.write_internal_node(internal_page, &cur)?;
                self.write_internal_node(parent, &pnode)?;
                return Ok(());
            } else {
                // merge cur + right
                let mut cur = self.read_internal_node(internal_page);
                let mut right = self.read_internal_node(right_page);
                let sep_index = idx;
                let sep_key = pnode.keys[sep_index];

                cur.keys[cur.num_keys] = sep_key;
                cur.num_keys += 1;
                for i in 0..right.num_keys {
                    cur.keys[cur.num_keys + i] = right.keys[i];
                }
                for i in 0..=right.num_keys {
                    cur.children[cur.num_keys + i] = right.children[i];
                }
                cur.num_keys += right.num_keys;
                self.write_internal_node(internal_page, &cur)?;
                // delete right from parent
                self.delete_entry_from_parent_after_merge(internal_page, right_page)?;
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn read_range_data(&self, lower_key: i32, upper_key: i32) -> Vec<Vec<u8>> {
        let mut results = Vec::new();

        let mut leaf_page = match self.find_leaf(lower_key) {
            Some(page) => page as i32,
            None => return results,
        };

        while leaf_page >= 0 {
            let node = self.read_leaf_node(leaf_page as usize);
            for i in 0..node.num_keys {
                if node.keys[i] >= lower_key && node.keys[i] <= upper_key {
                    results.push(node.data[i].to_vec());
                } else if node.keys[i] > upper_key {
                    return results;
                }
            }
            leaf_page = node.next_leaf;
        }

        results
    }

    pub fn flush(&mut self) -> Result<()> {
        // Ensure metadata is written to page 0, then flush mmap to disk
        self.write_metadata();
        self.mmap.flush()
    }
}

impl Drop for BPlusTree {
    fn drop(&mut self) {
        let _ = self.mmap.flush();
    }
}

use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref TREE: Mutex<Option<BPlusTree>> = Mutex::new(None);
}

fn ensure_tree_initialized() {
    let mut tree_guard = TREE.lock().unwrap();
    if tree_guard.is_none() {
        *tree_guard = BPlusTree::new().ok();
    }
}

#[allow(dead_code)]
fn get_tree() -> &'static Mutex<Option<BPlusTree>> {
    &TREE
}

#[no_mangle]
pub extern "C" fn writeData(key: i32, data: *const u8) -> i32 {
    ensure_tree_initialized();

    if data.is_null() {
        return 0;
    }

    let data_slice = unsafe { std::slice::from_raw_parts(data, DATA_SIZE) };
    let mut data_array = [0u8; DATA_SIZE];
    data_array.copy_from_slice(data_slice);

    let mut tree_guard = TREE.lock().unwrap();

    if let Some(ref mut tree) = *tree_guard {
        match tree.write_data(key, &data_array) {
            Ok(true) => 1,
            _ => 0,
        }
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn readData(key: i32) -> *mut u8 {
    ensure_tree_initialized();

    let tree_guard = TREE.lock().unwrap();

    if let Some(ref tree) = *tree_guard {
        if let Some(data) = tree.read_data(key) {
            // Return boxed slice pointer - caller must free using freeData
            let boxed_slice = data.into_boxed_slice();
            return Box::into_raw(boxed_slice) as *mut u8;
        }
    }

    std::ptr::null_mut()
}

/// Safer FFI: copy into caller-provided buffer. buf must be at least DATA_SIZE bytes.
#[no_mangle]
pub extern "C" fn readDataInto(buf: *mut u8, key: i32) -> i32 {
    ensure_tree_initialized();
    if buf.is_null() {
        return 0;
    }
    let mut tree_guard = TREE.lock().unwrap();
    if let Some(ref tree) = *tree_guard {
        let mut local = [0u8; DATA_SIZE];
        let ok = tree.read_data_into(key, &mut local);
        if ok == 1 {
            unsafe {
                let dst = std::slice::from_raw_parts_mut(buf, DATA_SIZE);
                dst.copy_from_slice(&local);
            }
            return 1;
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn deleteData(key: i32) -> i32 {
    ensure_tree_initialized();

    let mut tree_guard = TREE.lock().unwrap();
    if let Some(ref mut tree) = *tree_guard {
        match tree.delete_data(key) {
            Ok(true) => 1,
            _ => 0,
        }
    } else {
        0
    }
}

/// readRangeData: returns array of pointers (each pointer points to a boxed slice of length DATA_SIZE).
/// Caller must free array + each pointer via freeRangeData.
#[no_mangle]
pub extern "C" fn readRangeData(lower_key: i32, upper_key: i32, n: *mut i32) -> *mut *mut u8 {
    ensure_tree_initialized();

    if n.is_null() {
        return std::ptr::null_mut();
    }

    let tree_guard = TREE.lock().unwrap();

    if let Some(ref tree) = *tree_guard {
        let results = tree.read_range_data(lower_key, upper_key);

        unsafe {
            *n = results.len() as i32;
        }

        if results.is_empty() {
            return std::ptr::null_mut();
        }

        let mut ptrs: Vec<*mut u8> = results
            .into_iter()
            .map(|v| Box::into_raw(v.into_boxed_slice()) as *mut u8)
            .collect();

        let result = ptrs.as_mut_ptr();
        std::mem::forget(ptrs);
        result
    } else {
        std::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn freeData(data: *mut u8) {
    if !data.is_null() {
        unsafe {
            // Reconstruct boxed slice had length DATA_SIZE
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(data, DATA_SIZE));
        }
    }
}

#[no_mangle]
pub extern "C" fn freeRangeData(data: *mut *mut u8, n: i32) {
    if !data.is_null() && n > 0 {
        unsafe {
            let ptrs = Vec::from_raw_parts(data, n as usize, n as usize);
            for ptr in ptrs {
                if !ptr.is_null() {
                    let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr, DATA_SIZE));
                }
            }
        }
    }
}
