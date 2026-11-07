#[cfg(windows)]
pub fn pcsx2_running() -> Option<bool> {
    use std::{ffi::c_void, mem::size_of};
    #[repr(C)]
    struct Entry {
        size: u32, usage: u32, pid: u32, heap: usize, module: u32, threads: u32,
        parent: u32, priority: i32, flags: u32, executable: [u16; 260],
    }
    #[link(name="kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags:u32,pid:u32)->*mut c_void;
        fn Process32FirstW(snapshot:*mut c_void,entry:*mut Entry)->i32;
        fn Process32NextW(snapshot:*mut c_void,entry:*mut Entry)->i32;
        fn CloseHandle(handle:*mut c_void)->i32;
        fn GetLastError()->u32;
    }
    struct Snapshot(*mut c_void);
    impl Drop for Snapshot {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
    let raw=unsafe{CreateToolhelp32Snapshot(2,0)};
    if raw==(-1isize)as *mut c_void{return None;}
    let snapshot=Snapshot(raw);
    let mut entry:Entry=unsafe{std::mem::zeroed()};entry.size=size_of::<Entry>()as u32;
    if unsafe{Process32FirstW(snapshot.0,&mut entry)}==0{return None;}
    loop {
        let len=entry.executable.iter().position(|c|*c==0).unwrap_or(entry.executable.len());
        let name=String::from_utf16_lossy(&entry.executable[..len]).to_ascii_lowercase();
        if name.starts_with("pcsx2")&&name.ends_with(".exe"){return Some(true);}
        if unsafe{Process32NextW(snapshot.0,&mut entry)}==0{
            return if unsafe{GetLastError()}==18 {Some(false)} else {None};
        }
    }
}
