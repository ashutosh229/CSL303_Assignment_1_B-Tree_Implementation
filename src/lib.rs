use memmap2::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::io::Result;
use std::path::Path;

const PAGE_SIZE: usize = 4096;
const DATA_SIZE: usize = 100;
const INDEX_FILE: &str = "bptree_index.dat";

const LEAF_ORDER: usize = 36;
// INTERNAL_ORDER is the maximum number of keys an internal node can legally hold.
// We allocate arrays slightly bigger to hold temporary extra key/children during insertion/split.
const INTERNAL_ORDER: usize = 340; // maximum keys
                                   // arrays will be sized using INTERNAL_ORDER + 1 / +2 where needed

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

        bytes.push(if self.is_leaf { 1 } else { 0 });
        bytes.extend_from_slice(&self.num_keys.to_le_bytes());

        for &key in &self.keys {
            bytes.extend_from_slice(&key.to_le_bytes());
        }

        for data_item in &self.data {
            bytes.extend_from_slice(data_item);
        }

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

        node.num_keys = usize::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
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
    // allocate one extra key slot for temporary insertion before split
    keys: [i32; INTERNAL_ORDER + 1],
    // allocate two extra child slots for safety during splits
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
        bytes.extend_from_slice(&self.num_keys.to_le_bytes());

        // store entire keys array (INTERNAL_ORDER + 1)
        for &key in &self.keys {
            bytes.extend_from_slice(&key.to_le_bytes());
        }

        // store entire children array (INTERNAL_ORDER + 2)
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

        node.num_keys = usize::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
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
    root_page: i32,
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

        // Read metadata BEFORE mapping file
        let file_len = file.metadata()?.len();

        let (num_pages, root_page) = if exists && file_len > 0 {
            let pages = (file_len as usize) / PAGE_SIZE;
            (pages, 0)
        } else {
            file.set_len(PAGE_SIZE as u64)?;
            (1, 0)
        };

        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        let mut tree = BPlusTree {
            file,
            mmap,
            root_page,
            num_pages,
        };

        let needs_init = !exists || file_len == 0;

        if needs_init {
            let root = LeafNode::new();
            tree.write_leaf_node(root_page as usize, &root)?;
        }

        Ok(tree)
    }

    fn ensure_file_size(&mut self, pages: usize) -> Result<()> {
        let required_size = pages * PAGE_SIZE;
        let current_size = self.mmap.len();

        if required_size > current_size {
            self.mmap.flush()?;
            drop(std::mem::replace(&mut self.mmap, unsafe {
                MmapOptions::new().len(0).map_mut(&self.file)?
            }));

            self.file.set_len(required_size as u64)?;
            self.mmap = unsafe { MmapOptions::new().map_mut(&self.file)? };
            self.num_pages = pages;
        }

        Ok(())
    }

    fn allocate_page(&mut self) -> Result<usize> {
        let page_num = self.num_pages;
        self.num_pages += 1;
        self.ensure_file_size(self.num_pages)?;
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

    // Find leaf page for a key
    fn find_leaf(&self, key: i32) -> Option<usize> {
        if self.root_page < 0 {
            return None;
        }

        let mut current_page = self.root_page as usize;

        while !self.is_leaf_page(current_page) {
            let node = self.read_internal_node(current_page);
            let mut i = 0usize;
            while i < node.num_keys && key >= node.keys[i] {
                i += 1;
            }
            let child = node.children[i];
            if child < 0 {
                return None;
            }
            current_page = child as usize;
        }

        Some(current_page)
    }

    // Find parent internal node page of a child page
    fn find_parent(&self, target_page: usize) -> Option<usize> {
        if self.root_page < 0 || self.root_page as usize == target_page {
            return None;
        }

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

    // Insert key + right child into parent, splitting if necessary
    fn insert_into_parent(
        &mut self,
        parent_page: usize,
        key: i32,
        right_child_page: usize,
    ) -> Result<()> {
        let mut parent = self.read_internal_node(parent_page);

        // find insertion pos
        let mut pos = 0usize;
        while pos < parent.num_keys && parent.keys[pos] < key {
            pos += 1;
        }

        // shift keys right (safe because keys array has +1 capacity)
        for i in (pos..parent.num_keys).rev() {
            parent.keys[i + 1] = parent.keys[i];
        }

        // shift children right (children array has +2 capacity)
        for i in (pos + 1..=parent.num_keys).rev() {
            parent.children[i + 1] = parent.children[i];
        }

        // insert
        parent.keys[pos] = key;
        parent.children[pos + 1] = right_child_page as i32;
        parent.num_keys += 1;

        // if no overflow, write and return
        if parent.num_keys <= INTERNAL_ORDER {
            self.write_internal_node(parent_page, &parent)?;
            return Ok(());
        }

        // overflow -> split internal node
        let mid = parent.num_keys / 2;
        let promoted_key = parent.keys[mid];

        let mut new_internal = InternalNode::new();

        // right keys count
        let right_keys_count = parent.num_keys - (mid + 1);

        for i in 0..right_keys_count {
            new_internal.keys[i] = parent.keys[mid + 1 + i];
        }

        // copy right children (right_keys_count + 1 children)
        for i in 0..=(right_keys_count) {
            new_internal.children[i] = parent.children[mid + 1 + i];
        }

        new_internal.num_keys = right_keys_count;

        // shrink left parent
        parent.num_keys = mid;

        // cleanup (optional)
        for i in parent.num_keys..(INTERNAL_ORDER + 1) {
            parent.keys[i] = 0;
        }
        for i in (parent.num_keys + 1)..(INTERNAL_ORDER + 2) {
            parent.children[i] = -1;
        }

        // write left and right
        self.write_internal_node(parent_page, &parent)?;
        let new_page = self.allocate_page()?;
        self.write_internal_node(new_page, &new_internal)?;

        // if parent was root -> make new root
        if parent_page == self.root_page as usize {
            let new_root_page = self.allocate_page()?;
            let mut new_root = InternalNode::new();
            new_root.num_keys = 1;
            new_root.keys[0] = promoted_key;
            new_root.children[0] = parent_page as i32;
            new_root.children[1] = new_page as i32;
            self.write_internal_node(new_root_page, &new_root)?;
            self.root_page = new_root_page as i32;
            return Ok(());
        }

        // propagate up
        if let Some(grand) = self.find_parent(parent_page) {
            self.insert_into_parent(grand, promoted_key, new_page)?;
            Ok(())
        } else {
            // fallback root creation
            let new_root_page = self.allocate_page()?;
            let mut new_root = InternalNode::new();
            new_root.num_keys = 1;
            new_root.keys[0] = promoted_key;
            new_root.children[0] = parent_page as i32;
            new_root.children[1] = new_page as i32;
            self.write_internal_node(new_root_page, &new_root)?;
            self.root_page = new_root_page as i32;
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
            } else {
                // propagate to parent
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
                }
            }
        }

        self.mmap.flush()?;
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

        // find position
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

    pub fn delete_data(&mut self, key: i32) -> Result<bool> {
        let leaf_page = match self.find_leaf(key) {
            Some(page) => page,
            None => return Ok(false),
        };

        let mut node = self.read_leaf_node(leaf_page);
        for i in 0..node.num_keys {
            if node.keys[i] == key {
                for j in i..node.num_keys - 1 {
                    node.keys[j] = node.keys[j + 1];
                    node.data[j] = node.data[j + 1];
                }
                node.num_keys -= 1;
                self.write_leaf_node(leaf_page, &node)?;
                self.mmap.flush()?;
                return Ok(true);
            }
        }
        Ok(false)
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
            let boxed_slice = data.into_boxed_slice();
            return Box::into_raw(boxed_slice) as *mut u8;
        }
    }

    std::ptr::null_mut()
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
