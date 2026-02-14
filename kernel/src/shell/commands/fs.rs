// CantayaOS Shell — Filesystem Commands

extern crate alloc;

use alloc::string::String;
use crate::graphics::console;
use core::fmt::Write;

pub(crate) fn cmd_ls(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let path = if args.is_empty() { "." } else { args };
    // Convert backslashes if user typed Windows-style path
    let path_unix = crate::shell::win_to_unix_path(path);
    let entries = match vfs::list_dir(&path_unix) {
        Some(e) => e,
        None => {
            let mut s = String::new();
            write!(s, "File Not Found: {}", path).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    // Windows dir header: show the volume and directory path
    let display_path = if path_unix == "." {
        crate::shell::unix_to_win_path(&vfs::cwd())
    } else if path_unix.starts_with('/') {
        crate::shell::unix_to_win_path(&path_unix)
    } else {
        // Relative path — combine with CWD for display
        let cwd = vfs::cwd();
        let full = if cwd == "/" {
            let mut p = String::from("/");
            p.push_str(&path_unix);
            p
        } else {
            let mut p = cwd;
            p.push('/');
            p.push_str(&path_unix);
            p
        };
        crate::shell::unix_to_win_path(&full)
    };
    console::println("");
    console::println(" Volume in drive C is CANTAYAOS");
    console::println("");
    let mut s = String::new();
    write!(s, " Directory of {}", display_path).ok();
    console::println(&s);
    console::println("");

    if entries.is_empty() {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("File Not Found");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let mut file_count = 0u32;
    let mut dir_count = 0u32;
    let mut total_size: u64 = 0;

    for entry in &entries {
        let mut s = String::new();
        if entry.is_dir {
            dir_count += 1;
            console::set_color(0x55, 0xBB, 0xFF);
            write!(s, "    <DIR>          {}", entry.name).ok();
        } else {
            file_count += 1;
            total_size += entry.size as u64;
            console::set_color(0xFF, 0xFF, 0xFF);
            write!(s, "    {:>14}  {}", crate::shell::format_size_commas(entry.size), entry.name).ok();
        }
        console::println(&s);
    }
    console::set_color(0xFF, 0xFF, 0xFF);

    // Windows dir footer
    s.clear();
    write!(s, "    {:>8} File(s)  {:>14} bytes", file_count, crate::shell::format_size_commas(total_size as u32)).ok();
    console::println(&s);
    s.clear();
    write!(s, "    {:>8} Dir(s)", dir_count).ok();
    console::println(&s);
}

pub(crate) fn cmd_cat(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: cat <filename>");
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            if data.is_empty() {
                console::set_color(0xAA, 0xAA, 0xAA);
                console::println("(empty file)");
                console::set_color(0xFF, 0xFF, 0xFF);
                return;
            }

            // Display as text (with fallback for non-UTF8)
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    console::println(text);
                }
                Err(_) => {
                    // Show as hex dump for binary files
                    console::set_color(0xAA, 0xAA, 0xAA);
                    console::println("(binary file, showing hex dump)");
                    console::set_color(0xFF, 0xFF, 0xFF);
                    let limit = data.len().min(256);
                    for (i, chunk) in data[..limit].chunks(16).enumerate() {
                        let mut s = String::new();
                        write!(s, "  {:04X}: ", i * 16).ok();
                        for b in chunk {
                            write!(s, "{:02X} ", b).ok();
                        }
                        console::println(&s);
                    }
                    if data.len() > 256 {
                        let mut s = String::new();
                        write!(s, "  ... ({} more bytes)", data.len() - 256).ok();
                        console::println(&s);
                    }
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "cat: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_write(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    // Parse: write <filename> <content>
    let (filename, content) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: write <filename> <text>");
            return;
        }
    };

    if filename.is_empty() || content.is_empty() {
        console::println("Usage: write <filename> <text>");
        return;
    }

    if vfs::write_file(filename, content.as_bytes()) {
        let mut s = String::new();
        write!(s, "Wrote {} bytes to '{}'", content.len(), filename).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "write: failed to write to '{}'", filename).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_mkdir(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: mkdir <dirname>");
        return;
    }

    if vfs::mkdir(args) {
        let mut s = String::new();
        write!(s, "Created directory '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "mkdir: failed to create '{}'", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_rm(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: rm <filename|dirname>");
        return;
    }

    if vfs::delete(args) {
        let mut s = String::new();
        write!(s, "Deleted '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "rm: failed to delete '{}' (does it exist? is the directory empty?)", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_cp(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (src, dst) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: cp <source> <destination>");
            return;
        }
    };

    if src.is_empty() || dst.is_empty() {
        console::println("Usage: cp <source> <destination>");
        return;
    }

    if vfs::copy_file(src, dst) {
        let mut s = String::new();
        write!(s, "Copied '{}' -> '{}'", src, dst).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "cp: failed to copy '{}' to '{}'", src, dst).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_cd(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("The system cannot find the drive specified.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        // Like Windows cmd: cd with no args prints current directory
        cmd_pwd();
        return;
    }

    // Convert Windows-style backslashes to forward slashes for VFS
    let path = crate::shell::win_to_unix_path(args);

    if !vfs::cd(&path) {
        let mut s = String::new();
        write!(s, "The system cannot find the path specified: {}", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_pwd() {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    // Show Windows-style path
    let path = crate::shell::unix_to_win_path(&vfs::cwd());
    console::println(&path);
}

pub(crate) fn cmd_vol() {
    console::println("");
    console::println(" Volume in drive C is CANTAYAOS");
    console::println(" Volume Serial Number is CA17-AY05");
    console::println("");
}

pub(crate) fn cmd_disk() {
    use crate::storage::{vfs, fat32};
    use crate::hal::virtio_blk;

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Disk Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();

    if !virtio_blk::is_available() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("  No block device available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let sectors = virtio_blk::capacity_sectors();
    write!(s, "  Block device:   virtio-blk").ok();
    console::println(&s);

    s.clear();
    write!(s, "  Capacity:       {} sectors ({} MiB)", sectors, sectors * 512 / (1024 * 1024)).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Sector size:    512 bytes").ok();
    console::println(&s);

    if fat32::is_mounted() {
        let label = fat32::volume_label();
        s.clear();
        write!(s, "  Filesystem:     FAT32").ok();
        console::println(&s);

        s.clear();
        write!(s, "  Volume label:   {}", label).ok();
        console::println(&s);

        let (total, free, cluster_size) = fat32::stats();
        s.clear();
        write!(s, "  Cluster size:   {} bytes", cluster_size).ok();
        console::println(&s);

        let used = total - free;
        s.clear();
        write!(s, "  Clusters:       {} total, {} used, {} free", total, used, free).ok();
        console::println(&s);

        let free_bytes = free as u64 * cluster_size as u64;
        let used_bytes = used as u64 * cluster_size as u64;
        s.clear();
        write!(s, "  Space:          {} KiB used, {} KiB free",
            used_bytes / 1024, free_bytes / 1024).ok();
        console::println(&s);

        if vfs::is_ready() {
            s.clear();
            write!(s, "  Mount point:    /").ok();
            console::println(&s);
            s.clear();
            write!(s, "  Current dir:    {}", vfs::cwd()).ok();
            console::println(&s);
        }
    } else {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("  Filesystem:     (not mounted)");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_touch(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: touch <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if vfs::exists(args) {
        console::println("File already exists.");
        return;
    }

    if vfs::write_file(args, &[]) {
        let mut s = String::new();
        write!(s, "Created '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("touch: failed to create file");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_stat(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: stat <file|dir>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if !vfs::exists(args) {
        let mut s = String::new();
        write!(s, "stat: '{}': No such file or directory", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let mut s = String::new();
    console::set_color(0xFF, 0xFF, 0x55);
    write!(s, "  File: {}", args).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);

    if vfs::is_dir(args) {
        console::println("  Type: directory");
        if let Some(entries) = vfs::list_dir(args) {
            s.clear();
            write!(s, "  Contents: {} entries", entries.len()).ok();
            console::println(&s);
        }
    } else {
        console::println("  Type: regular file");
        if let Some(data) = vfs::read_file(args) {
            s.clear();
            write!(s, "  Size: {} bytes", data.len()).ok();
            console::println(&s);
            // Check if text or binary
            let is_text = data.iter().all(|&b| b == b'\n' || b == b'\r' || b == b'\t' || (b >= 0x20 && b < 0x7F));
            console::println(if is_text { "  Kind: text" } else { "  Kind: binary" });
        }
    }
}

pub(crate) fn cmd_tree(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let path = if args.is_empty() { "." } else { args };
    console::set_color(0x55, 0xBB, 0xFF);
    console::println(path);
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    fn print_tree(path: &str, prefix: &str, fc: &mut usize, dc: &mut usize) {
        use crate::storage::vfs;
        if let Some(entries) = vfs::list_dir(path) {
            let len = entries.len();
            for (i, entry) in entries.iter().enumerate() {
                let is_last = i == len - 1;
                let connector = if is_last { "└── " } else { "├── " };
                let mut line = String::new();
                write!(line, "{}{}", prefix, connector).ok();

                if entry.is_dir {
                    *dc += 1;
                    console::set_color(0x55, 0xBB, 0xFF);
                    write!(line, "{}/", entry.name).ok();
                    console::println(&line);
                    console::set_color(0xFF, 0xFF, 0xFF);

                    let new_prefix = alloc::format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                    let child_path = if path == "/" || path == "." {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", path, entry.name)
                    };
                    print_tree(&child_path, &new_prefix, fc, dc);
                } else {
                    *fc += 1;
                    write!(line, "{}", entry.name).ok();
                    console::println(&line);
                }
            }
        }
    }

    print_tree(path, "", &mut file_count, &mut dir_count);

    let mut s = String::new();
    console::set_color(0xAA, 0xAA, 0xAA);
    write!(s, "\n{} directories, {} files", dir_count, file_count).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
}

pub(crate) fn cmd_find(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let pattern = args.trim();
    if pattern.is_empty() {
        console::println("Usage: find <name-pattern>");
        return;
    }

    let pattern_lower = pattern.to_lowercase();
    let mut count = 0;

    fn search_recursive(path: &str, pattern: &str, count: &mut usize) {
        use crate::storage::vfs;
        if let Some(entries) = vfs::list_dir(path) {
            for entry in &entries {
                let full_path = if path == "/" {
                    alloc::format!("/{}", entry.name)
                } else {
                    alloc::format!("{}/{}", path, entry.name)
                };
                if entry.name.to_lowercase().contains(pattern) {
                    if entry.is_dir {
                        console::set_color(0x55, 0xBB, 0xFF);
                    } else {
                        console::set_color(0xFF, 0xFF, 0xFF);
                    }
                    console::println(&full_path);
                    *count += 1;
                }
                if entry.is_dir {
                    search_recursive(&full_path, pattern, count);
                }
            }
        }
    }

    search_recursive("/", &pattern_lower, &mut count);

    if count == 0 {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("(no files found)");
    } else {
        let mut s = String::new();
        console::set_color(0xAA, 0xAA, 0xAA);
        write!(s, "\n{} file(s) found", count).ok();
        console::println(&s);
    }
    console::set_color(0xFF, 0xFF, 0xFF);
}

pub(crate) fn cmd_head(args: &str) {
    use crate::storage::vfs;

    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        console::println("Usage: head <file> [n]");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = parts[0];
    let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);

    match vfs::read_file(filename) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("(binary)");
            for (i, line) in text.lines().enumerate() {
                if i >= count { break; }
                console::println(line);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "head: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_tail(args: &str) {
    use crate::storage::vfs;

    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        console::println("Usage: tail <file> [n]");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = parts[0];
    let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);

    match vfs::read_file(filename) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("(binary)");
            let lines: alloc::vec::Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(count);
            for line in &lines[start..] {
                console::println(line);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "tail: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_wc(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: wc <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("");
            let lines = text.lines().count();
            let words = text.split_whitespace().count();
            let chars = text.len();
            let mut s = String::new();
            write!(s, "  {:>6} lines  {:>6} words  {:>6} bytes  {}", lines, words, chars, args).ok();
            console::println(&s);
        }
        None => {
            let mut s = String::new();
            write!(s, "wc: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_grep(args: &str) {
    use crate::storage::vfs;

    let (pattern, filename) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: grep <pattern> <file>");
            return;
        }
    };

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(filename) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let pattern_lower = pattern.to_lowercase();
                    let mut count = 0;
                    for (i, line) in text.lines().enumerate() {
                        if line.to_lowercase().contains(&pattern_lower) {
                            let mut s = String::new();
                            console::set_color(0x55, 0xFF, 0x55);
                            write!(s, "{:>4}: ", i + 1).ok();
                            console::print(&s);
                            console::set_color(0xFF, 0xFF, 0xFF);
                            console::println(line);
                            count += 1;
                        }
                    }
                    if count == 0 {
                        console::set_color(0xAA, 0xAA, 0xAA);
                        console::println("(no matches)");
                        console::set_color(0xFF, 0xFF, 0xFF);
                    } else {
                        let mut s = String::new();
                        console::set_color(0xAA, 0xAA, 0xAA);
                        write!(s, "\n{} matching line(s)", count).ok();
                        console::println(&s);
                        console::set_color(0xFF, 0xFF, 0xFF);
                    }
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("grep: binary file, cannot search");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "grep: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_sort(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: sort <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let sorted = crate::shell::pipe_sort(text);
                    console::print(&sorted);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_uniq(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: uniq <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let result = crate::shell::pipe_uniq(text);
                    console::print(&result);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_more(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: more <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    crate::shell::pipe_more(text);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

pub(crate) fn cmd_append(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (filename, content) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: append <filename> <text>");
            return;
        }
    };

    // Read existing content, append, write back
    let mut existing = vfs::read_file(filename).unwrap_or_default();
    existing.extend_from_slice(content.as_bytes());
    existing.push(b'\n');

    if vfs::write_file(filename, &existing) {
        let mut s = String::new();
        write!(s, "Appended {} bytes to '{}'", content.len() + 1, filename).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("append: failed");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_rename(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (src, dst) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: rename <source> <destination>");
            return;
        }
    };

    // Rename = copy + delete
    if vfs::copy_file(src, dst) {
        if vfs::delete(src) {
            let mut s = String::new();
            write!(s, "Renamed '{}' -> '{}'", src, dst).ok();
            console::set_color(0x55, 0xFF, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        } else {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("rename: copied but failed to delete source");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("rename: failed to copy");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

pub(crate) fn cmd_xxd(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: xxd <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            let limit = data.len().min(512);
            for (i, chunk) in data[..limit].chunks(16).enumerate() {
                let mut s = String::new();
                console::set_color(0xAA, 0xAA, 0xAA);
                write!(s, "{:08X}: ", i * 16).ok();
                console::print(&s);
                s.clear();

                console::set_color(0xFF, 0xFF, 0xFF);
                for (j, b) in chunk.iter().enumerate() {
                    write!(s, "{:02X}", b).ok();
                    if j % 2 == 1 { write!(s, " ").ok(); }
                }
                // Pad if short
                for _ in chunk.len()..16 {
                    write!(s, "  ").ok();
                }
                write!(s, " ").ok();

                // ASCII representation
                console::set_color(0x55, 0xFF, 0x55);
                for b in chunk {
                    if *b >= 0x20 && *b < 0x7F {
                        write!(s, "{}", *b as char).ok();
                    } else {
                        write!(s, ".").ok();
                    }
                }
                console::println(&s);
            }
            console::set_color(0xFF, 0xFF, 0xFF);
            if data.len() > limit {
                let mut s = String::new();
                write!(s, "  ... ({} more bytes)", data.len() - limit).ok();
                console::set_color(0xAA, 0xAA, 0xAA);
                console::println(&s);
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "xxd: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}
