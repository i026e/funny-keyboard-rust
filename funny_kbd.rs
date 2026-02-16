// SPDX-License-Identifier: GPL-2.0

use kernel::prelude::*;
use kernel::{
    sync::{Arc, Mutex},
    workqueue::{self, Work, WorkItem},
    bindings,
};
use core::ffi::{c_char, c_int, c_ulong, c_uint, c_void};

module! {
    type: FunnyKeyboard,
    name: "funny_kbd",
    authors: ["Rust for Linux Contributors"],
    description: "Funny Kyeboard",
    license: "GPL",
}

// Const
const KEY_B: c_uint = 48;
const KEY_A: c_uint = 30;
const KEY_N: c_uint = 49;
const KEY_G: c_uint = 34;
const KEY_BANG: c_uint = 2;
const SYMB_BANG: u16 = 0xf021;
//const KEY_MAX: usize = 0x288;
//const KEY_CNT: usize = KEY_MAX + 1;
const KBD_KEYCODE: c_ulong = 0x01;
const NOTIFY_OK: c_int = 0x0001;
const GFP_ATOMIC: c_uint = 0x20;
const WQ_UNBOUND: c_uint = 0x02;
const BUS_VIRTUAL: u16 = 0x06;
const EV_KEY: c_uint = 0x01;
const EV_REP: c_uint = 0x14;
const EV_SYN: c_uint = 0x00;
const SYN_REPORT: c_uint = 0x00;

static ENGLISH_WORD: [c_uint; 4] = [KEY_B, KEY_A, KEY_N, KEY_G];

// Types
type KprobeOpcodeT = u8;
type KprobePreHandlerT = Option<unsafe extern "C" fn(*mut Kprobe, *mut c_void) -> c_int>;
type KprobePostHandlerT = Option<unsafe extern "C" fn(*mut Kprobe, *mut c_void, c_ulong)>;
type KallsymsLookupNameFn = unsafe extern "C" fn(*const c_char) -> c_ulong;

// Mut
static mut KEY_MAPS: *mut *mut u16 = core::ptr::null_mut();
static mut VIRT_KBD: *mut c_void = core::ptr::null_mut();
static mut KB_WQ: *mut c_void = core::ptr::null_mut();
static mut KBD_NB: NotifierBlock = NotifierBlock {
    notifier_call: Some(keyboard_notifier_cb),
    next: core::ptr::null_mut(),
    priority: 0,
};
static mut VIRT_KBD_NAME: &[u8] = b"Funny Keyboard\0";
static mut VIRT_KBD_PHYS: &[u8] = b"virtual/filtered/kbd\0";

// C functions
extern "C" {
    fn register_kprobe(kp: *mut Kprobe) -> c_int;
    fn unregister_kprobe(kp: *mut Kprobe);
    fn register_keyboard_notifier(nb: *mut NotifierBlock) -> c_int;
    fn unregister_keyboard_notifier(nb: *mut NotifierBlock) -> c_int;
    fn input_allocate_device() -> *mut c_void;
    fn input_register_device(dev: *mut c_void) -> c_int;
    fn input_unregister_device(dev: *mut c_void);
    fn input_free_device(dev: *mut c_void);
    fn input_event(dev: *mut c_void, type_: c_uint, code: c_uint, value: c_int);
    fn input_sync(dev: *mut c_void);
    fn alloc_workqueue_noprof(name: *const c_char, flags: c_uint, max_active: c_int) -> *mut c_void;
    fn destroy_workqueue(wq: *mut c_void);
    fn __flush_workqueue(wq: *mut c_void);
    fn __kmalloc_noprof(size: usize, flags: c_uint) -> *mut c_void;
    fn queue_work_node(node: c_int, wq: *mut c_void, work: *mut WorkStruct) -> bool;
    fn kfree(ptr: *mut c_void);
    fn __init_work(work: *mut WorkStruct, onstack: c_int);
    fn print_hex_dump(level: *const c_char, prefix_str: *const c_char, prefix_type: i32, rowsize: i32, groupsize: i32, buf: *const core::ffi::c_void, len: usize,ascii: bool, );

}

#[inline]
unsafe fn flush(wq: *mut c_void){
    unsafe {
        __flush_workqueue(wq);
    }

}

#[inline]
unsafe fn alloc_workqueue(name: *const c_char, flags: c_uint, max_active: c_int) ->  *mut c_void {
    unsafe {
        return alloc_workqueue_noprof(name, flags, max_active);
    }

}

#[inline]
unsafe fn kmalloc(size: usize, flags: c_uint) -> *mut c_void {
    unsafe {
        return __kmalloc_noprof(size, flags);
    }

}

#[inline]
unsafe fn queue_work(wq: *mut c_void, work: *mut WorkStruct) -> bool {
    unsafe {
        return queue_work_node(-1, wq, work);  // -1 means any CPU
    }

}

// Module
struct FunnyKeyboard;

impl kernel::Module for FunnyKeyboard {
    fn init(_module: &'static ThisModule) -> Result<Self> {
        pr_info!("funny-kbd: Module loaded\n");

        unsafe {
            KEY_MAPS = get_key_maps();
            pr_info!("KEY_MAPS: {KEY_MAPS:?}");

            KB_WQ = alloc_workqueue(b"kb_wq\0".as_ptr() as *const c_char, WQ_UNBOUND, 1);
            if KB_WQ.is_null() {
                pr_info!("Error allocate KB_WQ");
                return Err(ENOMEM);
            }
            pr_info!("KB_WQ: {KB_WQ:?}");
            init_virt_kbd();

            register_keyboard_notifier(&raw mut KBD_NB as *mut _);
        }



        Ok(FunnyKeyboard)
    }
}

impl Drop for FunnyKeyboard {
    fn drop(&mut self) {
        unsafe {
            unregister_keyboard_notifier(&raw mut KBD_NB as *mut _);

            if !VIRT_KBD.is_null() {
                input_unregister_device(VIRT_KBD);
                //input_free_device(VIRT_KBD);  // automatic
            }


            if !KB_WQ.is_null() {
                flush(KB_WQ);
                destroy_workqueue(KB_WQ);
            }


        }

        pr_info!("funny-kbd: Module unloaded\n");
    }
}

// Virtual Keyboard
// pahole -C input_dev
#[repr(C)]
struct InputDev {
    name: *const c_char,
    phys: *const c_char,
    uniq: *const c_char,
    id: InputId,
    propbit: [c_ulong; 1],
    evbit: [c_ulong; 1],
    keybit: [c_ulong; 12],
    // ... other fields omitted
}

// pahole -C input_id
#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}


#[inline]
unsafe fn set_bit(nr: usize, addr: *mut c_ulong) {
    unsafe {

        let word = addr.add(nr / (core::mem::size_of::<c_ulong>() * 8));
        let bit = nr % (core::mem::size_of::<c_ulong>() * 8);
        *word |= 1 << bit;
    }
}

unsafe fn send_keys(kbd: *mut c_void, keys: &[c_uint]) {
    unsafe {
        for &key in keys {
            input_event(kbd, EV_KEY, key, 1);  // key down
            input_event(kbd, EV_KEY, key, 0);  // key up
            input_event(kbd, EV_SYN, SYN_REPORT, 0);  // sync
        }
    }
}


unsafe fn init_virt_kbd() -> c_int {
    unsafe {
        // allocate
        VIRT_KBD = input_allocate_device();
        if VIRT_KBD.is_null() {
            pr_info!("Error allocating virtual keyboard");
            return -12; // -ENOMEM
        }
        pr_info!("VIRT_KBD: {VIRT_KBD:?}");

        // Set device properties via bindings
        let dev = &mut *(VIRT_KBD as *mut InputDev);

        dev.name = VIRT_KBD_NAME.as_ptr() as *const c_char;
        dev.phys = VIRT_KBD_PHYS.as_ptr() as *const c_char;
        dev.id.bustype = BUS_VIRTUAL;
        dev.id.vendor = 0xDEAD;
        dev.id.product = 0xBEEF;

        set_bit(EV_KEY as usize, dev.evbit.as_mut_ptr());
        set_bit(EV_REP as usize, dev.evbit.as_mut_ptr());

        // Set all bits to 1 for all keys
        /*
        for i in 0..=KEY_MAX {
            set_bit(i, dev.keybit.as_mut_ptr());
        }
        */
        dev.keybit = [!0; 12];

        // register
        let ret = input_register_device(VIRT_KBD);
        if ret != 0 {
            pr_info!("Error regestering virtual keyboard");
            input_free_device(VIRT_KBD);
            return ret;
        }

        return 0;
    };
}

// Notifier
#[repr(C)]
struct WorkStruct {
    data: c_ulong,
    entry: ListHead,
    func: Option<unsafe extern "C" fn(*mut WorkStruct)>,
}

#[repr(C)]
struct KbWork {
    work: WorkStruct,
    action: c_ulong,
    keycode: c_uint,
    shift: c_int,
}

unsafe fn init_work(work: *mut WorkStruct, func: unsafe extern "C" fn(*mut WorkStruct)) {
    unsafe {
        core::ptr::write_bytes(work, 0, 1);
        (*work).func = Some(func);
        (*work).entry.next = &mut (*work).entry as *mut ListHead;
        (*work).entry.prev = &mut (*work).entry as *mut ListHead;
        (*work).data = 0;
    }
}


unsafe extern "C" fn kb_work_fn(work: *mut WorkStruct) {
    unsafe {
        let kw = (work as *mut u8).sub(core::mem::offset_of!(KbWork, work)) as *mut KbWork;

        let maps = KEY_MAPS;
        if maps.is_null() {
            kfree(kw as *mut c_void);
            return;
        }

        let shift_map = *maps.offset((*kw).shift as isize);
        if shift_map.is_null() {
            kfree(kw as *mut c_void);
            return;
        }

        let symbol = *shift_map.offset((*kw).keycode as isize);

        pr_info!(
            "funny-kbd: keycode={} action={} shift={} symbol={:x}\n",
            (*kw).keycode,
                 (*kw).action,
                 (*kw).shift,
                 symbol
        );

        if (*kw).keycode == KEY_BANG && symbol == SYMB_BANG {
            pr_info!("BANG!!!\n");
            send_keys(VIRT_KBD, &ENGLISH_WORD);
        }

        kfree(kw as *mut c_void);
    }

}


unsafe extern "C" fn keyboard_notifier_cb(
    _nb: *mut NotifierBlock,
    action: c_ulong,
    param: *mut c_void,
) -> c_int {
    if action != KBD_KEYCODE {
        return NOTIFY_OK;
    }
    unsafe {
        let p = param as *mut KeyboardNotifierParam;
        if (*p).down == 0 {
            return NOTIFY_OK;
        }

        let keycode = (*p).value;
        let shift = (*p).shift;

        pr_info!("keycode: {keycode:?}; shift: {shift:?} \n");

        let kw = kmalloc(core::mem::size_of::<KbWork>(), GFP_ATOMIC) as *mut KbWork;
        if kw.is_null() {
            pr_info!("Failed to allocate work\n");
            return NOTIFY_OK;
        }
        //pr_info!("work allocated\n");

        init_work(&mut (*kw).work as *mut WorkStruct, kb_work_fn);

        (*kw).action = action;
        (*kw).keycode = keycode;
        (*kw).shift = shift;

        //pr_info!("work initialized\n");

        queue_work(KB_WQ, &mut (*kw).work as *mut WorkStruct);

        //pr_info!("work enqued\n");

        return NOTIFY_OK;
    }
}


// Key Map
#[repr(C)]
struct HlistNode {
    next: *mut *mut HlistNode,
    pprev: *mut *mut HlistNode,
}

#[repr(C)]
struct ListHead {
    next: *mut ListHead,
    prev: *mut ListHead,
}

#[repr(C)]
struct ArchSpecificInsn {
    insn: *mut c_void,
    boostable: bool,
    _pad: [u8; 7],
    size: c_int,
    _pad2: [u8; 20],
}

// pahole -C kprobe
#[repr(C)]
struct Kprobe {
    hlist: HlistNode,
    list: ListHead,
    nmissed: c_ulong,
    addr: *mut KprobeOpcodeT,
    symbol_name: *const c_char,
    offset: c_uint,
    _pad1: [u8; 4],
    pre_handler: KprobePreHandlerT,
    post_handler: KprobePostHandlerT,
    opcode: KprobeOpcodeT,
    _pad2: [u8; 7],
    ainsn: ArchSpecificInsn,
    flags: u32,
}
#[repr(C)]
struct KeyboardNotifierParam {
    vc: *mut c_void,
    down: c_int,
    shift: c_int,
    ledstate: c_int,
    value: c_uint,
}

#[repr(C)]
struct NotifierBlock {
    notifier_call: Option<unsafe extern "C" fn(*mut NotifierBlock, c_ulong, *mut c_void) -> c_int>,
    next: *mut NotifierBlock,
    priority: c_int,
}


unsafe fn kallsyms_lookup_name_hack(name: *const c_char) -> c_ulong {

    unsafe {
        let mut kp = core::mem::zeroed::<Kprobe>();
        kp.symbol_name = name;


        //dump_kprobe(&kp);

        if register_kprobe(&mut kp) < 0 {
            return 0;
        }

        //dump_kprobe(&kp);

        let addr = kp.addr as c_ulong;
        unregister_kprobe(&mut kp);

        return addr;
    };
}

unsafe fn get_key_maps() -> *mut *mut u16 {
    unsafe {
        let lookup_addr = kallsyms_lookup_name_hack(b"kallsyms_lookup_name\0".as_ptr() as *const c_char); //  b"...\0".as_ptr() == c_str!(...).as_char_ptr()
        if lookup_addr == 0 {
            return core::ptr::null_mut();
        }

        let lookup: KallsymsLookupNameFn = core::mem::transmute(lookup_addr);
        let key_maps_addr = lookup(b"key_maps\0".as_ptr() as *const c_char);

        if key_maps_addr == 0 {
            return core::ptr::null_mut();
        }
        return key_maps_addr as *mut *mut u16;
    };
}


// Debug

unsafe fn dump_kprobe(kp: *const Kprobe) {
    const DUMP_PREFIX_NONE: i32 = 0;
    unsafe {
        print_hex_dump(
            b"\0".as_ptr() as *const c_char,          // log level (empty -> default)
            b"kprobe: \n".as_ptr() as *const c_char,  // prefix
            DUMP_PREFIX_NONE,
            1024,                              // large row -> single line
            1,
            kp as *const _,
            core::mem::size_of::<Kprobe>(),
            false,
        );
    }
}
