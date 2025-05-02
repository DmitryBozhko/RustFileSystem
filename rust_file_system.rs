use std::collections::HashMap;

const BLOCK_SIZE: usize = 256;

#[derive(Clone, Debug, PartialEq)]
enum FileType {
    RegularFile,
    Directory,
}

#[derive(Clone, Debug)]
struct Inode {
    id: u64,
    name: String,
    size: u64,
    file_type: FileType,
    block_ids: Vec<u64>,
    entries: Option<Vec<u64>>,
}

#[derive(Clone, Debug)]
enum JournalOperation {
    CreateDirectory {dir_id: u64, name: String},
    CreateFile {file_id: u64, name: String},
    AddFileToDirectory {file_id: u64, dir_id: u64},
    WriteToFile {file_id: u64, old_data: Vec<u8>},
}

#[derive(Clone, Debug)]
struct JournalEntry {
    operation: JournalOperation,
    committed: bool,
}

struct Journal {
    entries: Vec<JournalEntry>,
}

impl Journal {
    fn new() -> Self {
        Self {entries: Vec::new()}
    }

    fn log(&mut self, operation: JournalOperation) {
        self.entries.push(JournalEntry {operation, committed: true});
    }

    fn pop_last(&mut self) -> Option<JournalOperation> {
        self.entries.pop().map(|entry| entry.operation)
    }

    fn print_journal(&self) {
        println!("Journal Entries:");
        for (i, entry) in self.entries.iter().enumerate() {
            let op_str = match &entry.operation {
                JournalOperation::CreateDirectory {dir_id, name} => {
                    format!("CREATE DIRECTORY: {} (ID: {})", name, dir_id)
                }
                JournalOperation::CreateFile {file_id, name} => {
                    format!("CREATE FILE: {} (ID: {})", name, file_id)
                }
                JournalOperation::AddFileToDirectory {file_id, dir_id} => {
                    format!("ADD FILE: {} TO DIRECTORY: {}", file_id, dir_id)
                }
                JournalOperation::WriteToFile {file_id, ..} => {
                    format!("WRITE TO FILE: {}", file_id)
                }
            };
            println!(" {}. {} [Committed: {}]", i + 1, op_str, entry.committed);
        }
    }
}

struct FileSystem {
    inodes: HashMap<u64, Inode>,
    blocks: HashMap<u64, [u8; BLOCK_SIZE]>,
    next_inode_id: u64,
    next_block_id: u64,
    journal: Journal,
}

impl FileSystem {
    fn new() -> Self {
        Self {
            inodes: HashMap::new(),
            blocks: HashMap::new(),
            next_inode_id: 1,
            next_block_id: 1,
            journal: Journal::new(),
        }
    }

    fn create_directory(&mut self, name: &str) -> u64 {
        let dir_id = self.next_inode_id;
        self.next_inode_id += 1;
        self.inodes.insert(
            dir_id,
            Inode {
                id: dir_id,
                name: name.to_string(),
                size: 0,
                file_type: FileType::Directory,
                block_ids: Vec::new(),
                entries: Some(Vec::new()),
            },
        );
        self.journal.log(JournalOperation::CreateDirectory {dir_id, name: name.to_string()});
        dir_id
    }

    fn create_file(&mut self, name: &str) -> u64 {
        let file_id = self.next_inode_id;
        self.next_inode_id += 1;
        self.inodes.insert(
            file_id,
            Inode {
                id: file_id,
                name: name.to_string(),
                size: 0,
                file_type: FileType::RegularFile,
                block_ids: Vec::new(),
                entries: None,
            },
        );
        self.journal.log(JournalOperation::CreateFile {file_id, name: name.to_string()});
        file_id
    }

    fn add_file_to_directory(&mut self, file_id: u64, dir_id: u64) {
        if let Some(dir_inode) = self.inodes.get_mut(&dir_id) {
            if dir_inode.file_type == FileType::Directory {
                if let Some(entries) = dir_inode.entries.as_mut() {
                    entries.push(file_id);
                    self.journal.log(JournalOperation::AddFileToDirectory {file_id, dir_id});
                }
            }
        }
    }

    fn write_to_file(&mut self, file_id: u64, new_data: &[u8]) {
        let old_data = {
            let data = self.read_file(file_id);
            data
        };
        let file_inode = match self.inodes.get_mut(&file_id) {
            Some(inode) if inode.file_type == FileType::RegularFile => inode,
            _ => return,
        };
        for b_id in &file_inode.block_ids {
            self.blocks.remove(b_id);
        }
        file_inode.block_ids.clear();
        let mut offset = 0;
        while offset < new_data.len() {
            let end = (offset + BLOCK_SIZE).min(new_data.len());
            let chunk = &new_data[offset..end];
            let block_id = self.next_block_id;
            self.next_block_id += 1;
            let mut block = [0u8; BLOCK_SIZE];
            block[..chunk.len()].copy_from_slice(chunk);
            self.blocks.insert(block_id, block);
            file_inode.block_ids.push(block_id);
            offset += BLOCK_SIZE;
        }
        file_inode.size = new_data.len() as u64;
        self.journal.log(JournalOperation::WriteToFile {
            file_id,
            old_data,
        });
    }

    fn read_file(&self, file_id: u64) -> Vec<u8> {
        let file_inode = match self.inodes.get(&file_id) {
            Some(inode) if inode.file_type == FileType::RegularFile => inode,
            _ => return vec![],
        };
        let mut result = Vec::new();
        for &block_id in &file_inode.block_ids {
            if let Some(block) = self.blocks.get(&block_id) {
                result.extend_from_slice(block);
            }
        }
        result.truncate(file_inode.size as usize);
        result
    }

    fn list_directories_and_files(&self) {
        for inode in self.inodes.values() {
            if inode.file_type == FileType::Directory {
                println!("Directory {} (ID: {}):", inode.name, inode.id);
                if let Some(entries) = &inode.entries {
                    for file_id in entries {
                        if let Some(file) = self.inodes.get(file_id) {
                            println!(
                                "  - File {} (ID: {}, Size: {} bytes)",
                                file.name, file.id, file.size
                            );
                        }
                    }
                }
            }
        }
    }

    fn undo(&mut self) -> Option<String> {
        let operation = self.journal.pop_last()?;
        match &operation {
            JournalOperation::CreateDirectory {dir_id, name: _} => {
                self.inodes.remove(dir_id);
                Some(format!("CREATE DIRECTORY: ID {} undone", dir_id))
            }
            JournalOperation::CreateFile {file_id, name: _} => {
                self.inodes.remove(file_id);
                Some(format!("CREATE FILE: ID {} undone", file_id))
            }
            JournalOperation::AddFileToDirectory {file_id, dir_id} => {
                if let Some(dir_inode) = self.inodes.get_mut(dir_id) {
                    if let Some(entries) = dir_inode.entries.as_mut() {
                        entries.retain(|&id| id != *file_id);
                    }
                }
                Some(format!("ADD FILE: {} TO DIRECTORY: {} undone", file_id, dir_id))
            }
            JournalOperation::WriteToFile {file_id, old_data} => {
                if let Some(inode) = self.inodes.get_mut(file_id) {
                    for b_id in &inode.block_ids {
                        self.blocks.remove(b_id);
                    }
                    inode.block_ids.clear();
                    let mut offset = 0;
                    while offset < old_data.len() {
                        let end = (offset + BLOCK_SIZE).min(old_data.len());
                        let chunk = &old_data[offset..end];
                        let block_id = self.next_block_id;
                        self.next_block_id += 1;
                        let mut block = [0u8; BLOCK_SIZE];
                        block[..chunk.len()].copy_from_slice(chunk);
                        self.blocks.insert(block_id, block);
                        inode.block_ids.push(block_id);
                        offset += BLOCK_SIZE;
                    }
                    inode.size = old_data.len() as u64;
                }
                Some(format!("WRITE TO FILE: {} undone", file_id))
            }
        }
    }
}

fn main() {
    let mut fs = FileSystem::new();

    let dir1 = fs.create_directory("Documents");
    let dir2 = fs.create_directory("Pictures");

    let file1 = fs.create_file("doc1.txt");
    let file2 = fs.create_file("doc2.txt");
    let file3 = fs.create_file("pic1.jpg");

    fs.add_file_to_directory(file1, dir1);
    fs.add_file_to_directory(file2, dir1);
    fs.add_file_to_directory(file3, dir2);

    fs.write_to_file(file1, b"Hello, world!");

    println!("\n=== Directory Listing ===");
    fs.list_directories_and_files();
    
    let data = fs.read_file(file1);
    println!("\n=== Read File ===");
    println!("File Data: {}", String::from_utf8_lossy(&data));

    println!("\n=== Journal ===");
    fs.journal.print_journal();

    println!("\n=== Undo Operation ===");
    if let Some(undone_operation) = fs.undo() {
        println!("Undid operation: {}", undone_operation);
    } else {
        println!("Nothing to undo.");
    }

    println!("\n=== Journal ===");
    fs.journal.print_journal();

    println!("\n=== Undo Operation ===");
    if let Some(undone_operation) = fs.undo() {
        println!("Undid operation: {}", undone_operation);
    } else {
        println!("Nothing to undo.");
    }

    println!("\n=== Final Journal ===");
    fs.journal.print_journal();
}