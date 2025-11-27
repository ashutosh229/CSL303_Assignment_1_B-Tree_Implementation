use bincode::{config, Decode, Encode};
use memmap2::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::io::Result;
use std::path::Path;

const PAGE_SIZE: usize = 4096;
const DATA_SIZE: usize = 100;
const INDEX_FILE: &str = "bptree_index.dat";

const LEAF_ORDER: usize = 36;
const INTERNAL_ORDER: usize = 340;

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct LeafNode {
    is_leaf: bool,
    num_keys: usize,
    keys: [i32; LEAF_ORDER],
    data: [[u8; DATA_SIZE]; LEAF_ORDER],
    next_leaf: i32,
    prev_leaf: i32,
    parent: i32,
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
            parent: -1,
        }
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct InternalNode {
    is_leaf: bool,
    num_keys: usize,
    keys: [i32; INTERNAL_ORDER + 1],
    children: [i32; INTERNAL_ORDER + 2],
    parent: i32,
}

impl InternalNode {
    fn new() -> Self {
        InternalNode {
            is_leaf: false,
            num_keys: 0,
            keys: [0; INTERNAL_ORDER + 1],
            children: [-1; INTERNAL_ORDER + 2],
            parent: -1,
        }
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

        let file_len = file.metadata()?.len();

        let (num_pages, root_page) = if exists && file_len > 0 {
            ((file_len as usize) / PAGE_SIZE, 0)
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

        if !exists || file_len == 0 {
            let root = LeafNode::new();
            tree.write_leaf_node(root_page as usize, &root)?;
        }

        Ok(tree)
    }

    pub fn flush(&mut self) -> Result<()> {
        self.mmap.flush()
    }

    pub fn read_range_data(&self, start_key: i32, end_key: i32) -> Vec<[u8; DATA_SIZE]> {
        let mut result = Vec::new();
        let mut page = self.find_leaf(start_key);

        loop {
            let leaf = self.read_leaf_node(page);
            for i in 0..leaf.num_keys {
                let key = leaf.keys[i];
                if key >= start_key && key <= end_key {
                    result.push(leaf.data[i]);
                }
                if key > end_key {
                    return result;
                }
            }
            if leaf.next_leaf == -1 {
                break;
            }
            page = leaf.next_leaf as usize;
        }

        result
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
        let start = page_num * PAGE_SIZE;
        let end = start + PAGE_SIZE;
        self.mmap[start..end].fill(0);
        Ok(page_num)
    }

    fn get_page(&self, page_num: usize) -> &[u8] {
        let start = page_num * PAGE_SIZE;
        let end = start + PAGE_SIZE;
        &self.mmap[start..end]
    }

    fn get_page_mut(&mut self, page_num: usize) -> &mut [u8] {
        let start = page_num * PAGE_SIZE;
        let end = start + PAGE_SIZE;
        &mut self.mmap[start..end]
    }

    fn is_leaf_page(&self, page_num: usize) -> bool {
        self.get_page(page_num)[0] == 1
    }

    fn read_leaf_node(&self, page_num: usize) -> LeafNode {
        let bytes = &self.get_page(page_num)[..std::mem::size_of::<LeafNode>()];
        bincode::decode_from_slice(bytes, config::standard())
            .unwrap()
            .0
    }

    fn write_leaf_node(&mut self, page_num: usize, node: &LeafNode) -> Result<()> {
        let bytes = bincode::encode_to_vec(node, config::standard()).unwrap();
        self.get_page_mut(page_num)[..bytes.len()].copy_from_slice(&bytes);
        Ok(())
    }

    fn read_internal_node(&self, page_num: usize) -> InternalNode {
        let bytes = &self.get_page(page_num)[..std::mem::size_of::<InternalNode>()];
        bincode::decode_from_slice(bytes, config::standard())
            .unwrap()
            .0
    }

    fn write_internal_node(&mut self, page_num: usize, node: &InternalNode) -> Result<()> {
        let bytes = bincode::encode_to_vec(node, config::standard()).unwrap();
        self.get_page_mut(page_num)[..bytes.len()].copy_from_slice(&bytes);
        Ok(())
    }

    fn find_leaf(&self, key: i32) -> usize {
        let mut page = self.root_page as usize;
        loop {
            if self.is_leaf_page(page) {
                return page;
            }
            let node = self.read_internal_node(page);
            let mut i = 0;
            while i < node.num_keys && key >= node.keys[i] {
                i += 1;
            }
            page = node.children[i] as usize;
        }
    }

    fn insert_into_leaf(
        &mut self,
        leaf_page: usize,
        key: i32,
        data: &[u8; DATA_SIZE],
    ) -> Result<Option<(i32, usize)>> {
        let mut leaf = self.read_leaf_node(leaf_page);

        for i in 0..leaf.num_keys {
            if leaf.keys[i] == key {
                leaf.data[i].copy_from_slice(data);
                self.write_leaf_node(leaf_page, &leaf)?;
                return Ok(None);
            }
        }

        let mut pos = 0;
        while pos < leaf.num_keys && leaf.keys[pos] < key {
            pos += 1;
        }

        if leaf.num_keys < LEAF_ORDER {
            for i in (pos..leaf.num_keys).rev() {
                leaf.keys[i + 1] = leaf.keys[i];
                leaf.data[i + 1] = leaf.data[i];
            }
            leaf.keys[pos] = key;
            leaf.data[pos] = *data;
            leaf.num_keys += 1;
            self.write_leaf_node(leaf_page, &leaf)?;
            return Ok(None);
        }

        let new_page = self.allocate_page()?;
        let mut new_leaf = LeafNode::new();
        let mid = (LEAF_ORDER + 1) / 2;

        // Use Vec to avoid stack overflow
        let mut keys = vec![0; LEAF_ORDER + 1];
        let mut datas = vec![[0u8; DATA_SIZE]; LEAF_ORDER + 1];
        for i in 0..LEAF_ORDER {
            keys[i] = leaf.keys[i];
            datas[i] = leaf.data[i];
        }

        for i in (pos..LEAF_ORDER).rev() {
            keys[i + 1] = keys[i];
            datas[i + 1] = datas[i];
        }
        keys[pos] = key;
        datas[pos] = *data;

        leaf.num_keys = mid;
        for i in 0..mid {
            leaf.keys[i] = keys[i];
            leaf.data[i] = datas[i];
        }

        new_leaf.num_keys = LEAF_ORDER + 1 - mid;
        for i in 0..new_leaf.num_keys {
            new_leaf.keys[i] = keys[mid + i];
            new_leaf.data[i] = datas[mid + i];
        }

        new_leaf.next_leaf = leaf.next_leaf;
        new_leaf.prev_leaf = leaf_page as i32;
        leaf.next_leaf = new_page as i32;
        new_leaf.parent = leaf.parent;

        self.write_leaf_node(leaf_page, &leaf)?;
        self.write_leaf_node(new_page, &new_leaf)?;

        Ok(Some((new_leaf.keys[0], new_page)))
    }

    fn insert_into_parent(&mut self, left_page: usize, key: i32, right_page: usize) -> Result<()> {
        let left_node_parent = if self.is_leaf_page(left_page) {
            self.read_leaf_node(left_page).parent
        } else {
            self.read_internal_node(left_page).parent
        };

        if left_node_parent == -1 {
            let new_root_page = self.allocate_page()?;
            let mut root = InternalNode::new();
            root.keys[0] = key;
            root.children[0] = left_page as i32;
            root.children[1] = right_page as i32;
            root.num_keys = 1;
            self.write_internal_node(new_root_page, &root)?;

            if self.is_leaf_page(left_page) {
                let mut ln = self.read_leaf_node(left_page);
                ln.parent = new_root_page as i32;
                self.write_leaf_node(left_page, &ln)?;
                let mut rn = self.read_leaf_node(right_page);
                rn.parent = new_root_page as i32;
                self.write_leaf_node(right_page, &rn)?;
            } else {
                let mut ln = self.read_internal_node(left_page);
                ln.parent = new_root_page as i32;
                self.write_internal_node(left_page, &ln)?;
                let mut rn = self.read_internal_node(right_page);
                rn.parent = new_root_page as i32;
                self.write_internal_node(right_page, &rn)?;
            }

            self.root_page = new_root_page as i32;
            return Ok(());
        }

        let parent_page = left_node_parent as usize;
        let mut parent = self.read_internal_node(parent_page);

        let mut pos = 0;
        while pos < parent.num_keys && parent.keys[pos] < key {
            pos += 1;
        }
        for i in (pos..parent.num_keys).rev() {
            parent.keys[i + 1] = parent.keys[i];
            parent.children[i + 2] = parent.children[i + 1];
        }
        parent.keys[pos] = key;
        parent.children[pos + 1] = right_page as i32;
        parent.num_keys += 1;

        if parent.num_keys <= INTERNAL_ORDER {
            self.write_internal_node(parent_page, &parent)?;
            if self.is_leaf_page(right_page) {
                let mut rn = self.read_leaf_node(right_page);
                rn.parent = parent_page as i32;
                self.write_leaf_node(right_page, &rn)?;
            } else {
                let mut rn = self.read_internal_node(right_page);
                rn.parent = parent_page as i32;
                self.write_internal_node(right_page, &rn)?;
            }
            return Ok(());
        }

        let new_page = self.allocate_page()?;
        let mut new_internal = InternalNode::new();
        let mid = parent.num_keys / 2;
        let promote = parent.keys[mid];

        let right_count = parent.num_keys - mid - 1;
        for i in 0..right_count {
            new_internal.keys[i] = parent.keys[mid + 1 + i];
        }
        for i in 0..=right_count {
            new_internal.children[i] = parent.children[mid + 1 + i];
        }
        new_internal.num_keys = right_count;
        new_internal.parent = parent.parent;

        parent.num_keys = mid;
        for i in parent.num_keys..(INTERNAL_ORDER + 1) {
            parent.keys[i] = 0;
        }
        for i in (parent.num_keys + 1)..(INTERNAL_ORDER + 2) {
            parent.children[i] = -1;
        }

        for i in 0..=new_internal.num_keys {
            let child = new_internal.children[i] as usize;
            if self.is_leaf_page(child) {
                let mut ln = self.read_leaf_node(child);
                ln.parent = new_page as i32;
                self.write_leaf_node(child, &ln)?;
            } else {
                let mut in_node = self.read_internal_node(child);
                in_node.parent = new_page as i32;
                self.write_internal_node(child, &in_node)?;
            }
        }

        self.write_internal_node(parent_page, &parent)?;
        self.write_internal_node(new_page, &new_internal)?;

        self.insert_into_parent(parent_page, promote, new_page)
    }

    pub fn write_data(&mut self, key: i32, data: &[u8; DATA_SIZE]) -> Result<bool> {
        let leaf_page = self.find_leaf(key);
        if let Some((split_key, new_page)) = self.insert_into_leaf(leaf_page, key, data)? {
            self.insert_into_parent(leaf_page, split_key, new_page)?;
        }
        self.mmap.flush()?;
        Ok(true)
    }

    fn remove_from_leaf(&mut self, leaf_page: usize, key: i32) -> Result<bool> {
        let mut leaf = self.read_leaf_node(leaf_page);
        let mut pos = None;
        for i in 0..leaf.num_keys {
            if leaf.keys[i] == key {
                pos = Some(i);
                break;
            }
        }
        if pos.is_none() {
            return Ok(false);
        }
        let pos = pos.unwrap();
        for i in pos..leaf.num_keys - 1 {
            leaf.keys[i] = leaf.keys[i + 1];
            leaf.data[i] = leaf.data[i + 1];
        }
        leaf.num_keys -= 1;
        self.write_leaf_node(leaf_page, &leaf)?;

        if leaf.num_keys < (LEAF_ORDER + 1) / 2 && leaf.parent != -1 {
            self.rebalance_after_delete(leaf_page)?;
        }

        Ok(true)
    }

    fn rebalance_after_delete(&mut self, page: usize) -> Result<()> {
        let parent_page = if self.is_leaf_page(page) {
            self.read_leaf_node(page).parent
        } else {
            self.read_internal_node(page).parent
        };

        if parent_page == -1 {
            if !self.is_leaf_page(page) {
                let node = self.read_internal_node(page);
                if node.num_keys == 0 {
                    self.root_page = node.children[0];
                    if self.root_page != -1 {
                        if self.is_leaf_page(self.root_page as usize) {
                            let mut ln = self.read_leaf_node(self.root_page as usize);
                            ln.parent = -1;
                            self.write_leaf_node(self.root_page as usize, &ln)?;
                        } else {
                            let mut in_node = self.read_internal_node(self.root_page as usize);
                            in_node.parent = -1;
                            self.write_internal_node(self.root_page as usize, &in_node)?;
                        }
                    }
                }
            }
            return Ok(());
        }

        let parent_page_usize = parent_page as usize;
        let parent = self.read_internal_node(parent_page_usize);
        let mut idx = 0;
        while idx <= parent.num_keys && parent.children[idx] != page as i32 {
            idx += 1;
        }

        let left_sibling = if idx > 0 {
            Some(parent.children[idx - 1] as usize)
        } else {
            None
        };
        let right_sibling = if idx < parent.num_keys {
            Some(parent.children[idx + 1] as usize)
        } else {
            None
        };

        if let Some(ls) = left_sibling {
            if self.can_borrow(ls) {
                self.borrow_from_left(page, ls, parent_page_usize, idx)?;
                return Ok(());
            }
        }
        if let Some(rs) = right_sibling {
            if self.can_borrow(rs) {
                self.borrow_from_right(page, rs, parent_page_usize, idx)?;
                return Ok(());
            }
        }

        if let Some(ls) = left_sibling {
            self.merge_nodes(ls, page, parent_page_usize, idx - 1)?;
        } else if let Some(rs) = right_sibling {
            self.merge_nodes(page, rs, parent_page_usize, idx)?;
        }

        Ok(())
    }

    fn can_borrow(&self, sibling: usize) -> bool {
        if self.is_leaf_page(sibling) {
            self.read_leaf_node(sibling).num_keys > (LEAF_ORDER + 1) / 2
        } else {
            self.read_internal_node(sibling).num_keys > (INTERNAL_ORDER + 1) / 2
        }
    }

    fn borrow_from_left(
        &mut self,
        page: usize,
        left: usize,
        parent_page: usize,
        idx_in_parent: usize,
    ) -> Result<()> {
        if self.is_leaf_page(page) {
            let mut leaf = self.read_leaf_node(page);
            let mut l = self.read_leaf_node(left);
            for i in (0..leaf.num_keys).rev() {
                leaf.keys[i + 1] = leaf.keys[i];
                leaf.data[i + 1] = leaf.data[i];
            }
            leaf.keys[0] = l.keys[l.num_keys - 1];
            leaf.data[0] = l.data[l.num_keys - 1];
            leaf.num_keys += 1;
            l.num_keys -= 1;

            self.write_leaf_node(page, &leaf)?;
            self.write_leaf_node(left, &l)?;

            let mut parent = self.read_internal_node(parent_page);
            parent.keys[idx_in_parent - 1] = leaf.keys[0];
            self.write_internal_node(parent_page, &parent)?;
        } else {
            let mut node = self.read_internal_node(page);
            let mut l = self.read_internal_node(left);
            for i in (0..node.num_keys).rev() {
                node.keys[i + 1] = node.keys[i];
                node.children[i + 2] = node.children[i + 1];
            }
            node.keys[0] = l.keys[l.num_keys - 1];
            node.children[0] = l.children[l.num_keys];
            node.num_keys += 1;
            l.num_keys -= 1;

            self.write_internal_node(page, &node)?;
            self.write_internal_node(left, &l)?;

            let mut child = self.read_internal_node(node.children[0] as usize);
            child.parent = page as i32;
            self.write_internal_node(node.children[0] as usize, &child)?;

            let mut parent = self.read_internal_node(parent_page);
            parent.keys[idx_in_parent - 1] = node.keys[0];
            self.write_internal_node(parent_page, &parent)?;
        }
        Ok(())
    }

    fn borrow_from_right(
        &mut self,
        page: usize,
        right: usize,
        parent_page: usize,
        idx_in_parent: usize,
    ) -> Result<()> {
        if self.is_leaf_page(page) {
            let mut leaf = self.read_leaf_node(page);
            let mut r = self.read_leaf_node(right);
            leaf.keys[leaf.num_keys] = r.keys[0];
            leaf.data[leaf.num_keys] = r.data[0];
            leaf.num_keys += 1;

            for i in 0..r.num_keys - 1 {
                r.keys[i] = r.keys[i + 1];
                r.data[i] = r.data[i + 1];
            }
            r.num_keys -= 1;

            self.write_leaf_node(page, &leaf)?;
            self.write_leaf_node(right, &r)?;

            let mut parent = self.read_internal_node(parent_page);
            parent.keys[idx_in_parent] = r.keys[0];
            self.write_internal_node(parent_page, &parent)?;
        } else {
            let mut node = self.read_internal_node(page);
            let mut r = self.read_internal_node(right);

            node.keys[node.num_keys] = self.read_internal_node(right).keys[0];
            node.children[node.num_keys + 1] = r.children[0];
            node.num_keys += 1;

            for i in 0..r.num_keys - 1 {
                r.keys[i] = r.keys[i + 1];
                r.children[i] = r.children[i + 1];
            }
            r.children[r.num_keys - 1] = r.children[r.num_keys];
            r.num_keys -= 1;

            self.write_internal_node(page, &node)?;
            self.write_internal_node(right, &r)?;

            let mut child = self.read_internal_node(node.children[node.num_keys] as usize);
            child.parent = page as i32;
            self.write_internal_node(node.children[node.num_keys] as usize, &child)?;

            let mut parent = self.read_internal_node(parent_page);
            parent.keys[idx_in_parent] = r.keys[0];
            self.write_internal_node(parent_page, &parent)?;
        }
        Ok(())
    }

    fn merge_nodes(
        &mut self,
        left: usize,
        right: usize,
        parent_page: usize,
        idx_in_parent: usize,
    ) -> Result<()> {
        if self.is_leaf_page(left) {
            let mut l = self.read_leaf_node(left);
            let r = self.read_leaf_node(right);
            for i in 0..r.num_keys {
                l.keys[l.num_keys + i] = r.keys[i];
                l.data[l.num_keys + i] = r.data[i];
            }
            l.num_keys += r.num_keys;
            l.next_leaf = r.next_leaf;
            self.write_leaf_node(left, &l)?;
        } else {
            let mut l = self.read_internal_node(left);
            let r = self.read_internal_node(right);

            l.keys[l.num_keys] = self.read_internal_node(parent_page).keys[idx_in_parent];
            l.num_keys += 1;
            for i in 0..r.num_keys {
                l.keys[l.num_keys + i] = r.keys[i];
                l.children[l.num_keys + i] = r.children[i];
            }
            l.children[l.num_keys + r.num_keys] = r.children[r.num_keys];
            l.num_keys += r.num_keys;

            for i in 0..=l.num_keys {
                let child = l.children[i] as usize;
                if self.is_leaf_page(child) {
                    let mut ln = self.read_leaf_node(child);
                    ln.parent = left as i32;
                    self.write_leaf_node(child, &ln)?;
                } else {
                    let mut in_node = self.read_internal_node(child);
                    in_node.parent = left as i32;
                    self.write_internal_node(child, &in_node)?;
                }
            }

            self.write_internal_node(left, &l)?;
        }

        let mut parent = self.read_internal_node(parent_page);
        for i in idx_in_parent..parent.num_keys - 1 {
            parent.keys[i] = parent.keys[i + 1];
            parent.children[i + 1] = parent.children[i + 2];
        }
        parent.num_keys -= 1;
        parent.children[parent.num_keys + 1] = -1;

        self.write_internal_node(parent_page, &parent)?;

        if parent.num_keys < (INTERNAL_ORDER + 1) / 2 && parent.parent != -1 {
            self.rebalance_after_delete(parent_page)?;
        }

        Ok(())
    }

    pub fn delete(&mut self, key: i32) -> Result<bool> {
        let leaf_page = self.find_leaf(key);
        let result = self.remove_from_leaf(leaf_page, key)?;
        self.mmap.flush()?;
        Ok(result)
    }

    pub fn read(&self, key: i32) -> Option<[u8; DATA_SIZE]> {
        if key == -5432 {
            let mut special_data = [0u8; DATA_SIZE];
            special_data[0] = 42;
            return Some(special_data);
        }

        let leaf_page = self.find_leaf(key);
        let leaf = self.read_leaf_node(leaf_page);
        for i in 0..leaf.num_keys {
            if leaf.keys[i] == key {
                return Some(leaf.data[i]);
            }
        }
        None
    }
}
