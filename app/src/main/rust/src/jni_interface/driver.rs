//! JNI methods for WuwaDriver

use crate::core::{MemoryAccessMode, DRIVER_MANAGER};
use crate::ext::jni::{JniResult, JniResultExt};
use crate::wuwa::{WuWaDriver, WuwaMemRegionEntry};
use anyhow::anyhow;
use jni::JNIEnv;
use jni::objects::{JClass, JIntArray, JObject, JObjectArray};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jlong, jsize};
use jni_macro::jni_method;
use log::{debug, error, info};
use nix::libc::close;
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap};
use obfstr::obfstr as s;
use std::num::NonZeroUsize;

mod conversions {
    use super::*;
    use crate::wuwa::WuwaGetProcInfoCmd;

    /// 从C风格字符串数组中提取UTF-8字符串
    pub fn extract_cstring(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
        String::from_utf8(bytes[0..end].to_vec()).unwrap_or_default()
    }

    /// 将ProcessInfo转换为JObject
    pub fn proc_info_to_jobject<'l>(
        env: &mut JNIEnv<'l>,
        proc_info: &WuwaGetProcInfoCmd,
    ) -> JniResult<JObject<'l>> {
        let process_info_class = env.find_class("moe/fuqiuluo/mamu/driver/CProcInfo")?;

        let cmdline = extract_cstring(&proc_info.name);
        let cmdline_jni_str = env.new_string(&cmdline)?;

        Ok(env.new_object(
            process_info_class,
            "(IILjava/lang/String;IIIJ)V",
            &[
                (proc_info.pid as jint).into(),
                (proc_info.tgid as jint).into(),
                (&cmdline_jni_str).into(),
                (proc_info.uid as jint).into(),
                (proc_info.ppid as jint).into(),
                (proc_info.prio as jint).into(),
                (proc_info.rss as jlong).into(),
            ],
        )?)
    }

    /// 将MemRegionEntry转换为JObject
    pub fn mem_region_to_jobject<'l>(
        env: &mut JNIEnv<'l>,
        entry: &WuwaMemRegionEntry,
        mem_region_class: &JClass<'l>,
    ) -> JniResult<JObject<'l>> {
        let name = extract_cstring(&entry.name);
        let jname = env.new_string(&name)?;

        Ok(env.new_object(
            mem_region_class,
            "(JJILjava/lang/String;)V",
            &[
                (entry.start as jlong).into(),
                (entry.end as jlong).into(),
                (entry.type_ as jint).into(),
                (&jname).into(),
            ],
        )?)
    }
}

// Core driver setup JNI methods

#[jni_method(90, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeSetDriverFd", "(I)Z")]
pub fn jni_set_driver_fd(mut env: JNIEnv, _obj: JObject, fd: i32) -> jboolean {
    (|| -> JniResult<jboolean> {
        if DRIVER_MANAGER.is_poisoned() {
            return Err(anyhow!("DriverManager is poisoned"));
        }

        let mut manager = DRIVER_MANAGER
            .write()
            .map_err(|_| anyhow!("Failed to acquire DriverManager write lock"))?;

        if !manager.is_driver_loaded() {
            manager.set_driver(WuWaDriver::from_fd(fd));
            debug!("{}: {}, {}", s!("设置驱动文件描述符"), fd, s!("驱动已初始化"));
        }

        if let Some(driver) = manager.get_driver() {
            let Ok(proc_info) = (unsafe { driver.get_process_info(nix::libc::getpid()) }) else {
                return Err(anyhow!("Failed to get process info"));
            };
            let split_index = proc_info
                .name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(proc_info.name.len());
            let cmdline = String::from_utf8(proc_info.name[0..split_index].to_vec()).unwrap_or_default();
            if !cmdline.contains(s!("fuqiuluo")) {
                return Err(anyhow!("Current process name verification failed"));
            }

            debug!("{}: {}", s!("驱动初始化成功，当前进程名称"), cmdline);
        } else {
            return Err(anyhow!("Failed to initialize driver"));
        }

        Ok(JNI_TRUE)
    })()
    .or_throw(&mut env)
}

#[jni_method(90, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeIsLoaded", "()Z")]
pub fn jni_is_loaded(_env: JNIEnv, _obj: JObject) -> jboolean {
    if let Ok(manager) = DRIVER_MANAGER.read() {
        if manager.is_driver_loaded() {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    } else {
        JNI_FALSE
    }
}

#[jni_method(90, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeSetMemoryAccessMode", "(I)V")]
pub fn jni_set_memory_access_mode(mut env: JNIEnv, _obj: JObject, mode_id: i32) {
    (|| -> JniResult<()> {
        if DRIVER_MANAGER.is_poisoned() {
            return Err(anyhow!("DriverManager is poisoned"));
        }
        let mut manager = DRIVER_MANAGER
            .write()
            .map_err(|_| anyhow!("Failed to acquire DriverManager write lock"))?;
        let mode =
            MemoryAccessMode::from_id(mode_id).ok_or_else(|| anyhow!("Invalid memory access mode id: {}", mode_id))?;
        manager.set_access_mode(mode)?;
        debug!("{}: {}, {}", s!("设置内存访问模式"), mode_id, format!("{:?}", mode));
        Ok(())
    })()
    .or_throw(&mut env)
}

// Process management JNI methods

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeIsProcessAlive", "(I)Z")]
pub fn jni_is_proc_alive(mut env: JNIEnv, _obj: JObject, pid: jint) -> jboolean {
    (|| -> JniResult<jboolean> {
        let manager = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;

        if let Some(driver) = manager.get_driver() {
            if let Ok(alive) = driver.is_process_alive(pid) {
                if alive {
                    return Ok(JNI_TRUE);
                }
            }
        }
        Ok(JNI_FALSE)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeGetProcessList", "()[I")]
pub fn jni_get_proc_list<'l>(mut env: JNIEnv<'l>, _obj: JObject) -> JIntArray<'l> {
    (|| -> JniResult<JIntArray<'l>> {
        let manager = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;

        let driver = manager.get_driver()
            .ok_or_else(|| anyhow!("Driver is not initialized"))?;

        let proc_list = driver.list_processes();
        let result = env.new_int_array(proc_list.len() as jsize)
            .map_err(|_| anyhow!("Cannot create process list result array"))?;
        env.set_int_array_region(&result, 0, &proc_list)?;
        Ok(result)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeGetProcessInfo", "(I)Lmoe/fuqiuluo/mamu/driver/CProcInfo;")]
pub fn jni_get_proc_info<'l>(mut env: JNIEnv<'l>, _obj: JObject, pid: jint) -> JObject<'l> {
    (|| -> JniResult<JObject<'l>> {
        let manager = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;
        let driver = manager.get_driver()
            .ok_or_else(|| anyhow!("Driver is not initialized"))?;

        let proc_info = driver
            .get_process_info(pid)
            .map_err(|_| anyhow!("Unable to get process info for pid {}", pid))?;

        conversions::proc_info_to_jobject(&mut env, &proc_info)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeGetProcessListWithInfo", "()[Lmoe/fuqiuluo/mamu/driver/CProcInfo;")]
pub fn jni_get_proc_list_with_info<'l>(mut env: JNIEnv<'l>, _obj: JObject) -> JObjectArray<'l> {
    (|| -> JniResult<JObjectArray<'l>> {
        let manager = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;
        let driver = manager.get_driver()
            .ok_or_else(|| anyhow!("Driver is not initialized"))?;

        let proc_list = driver.list_processes();
        let process_info_class = env.find_class("moe/fuqiuluo/mamu/driver/CProcInfo")?;
        let result_array = env.new_object_array(proc_list.len() as jsize, &process_info_class, JObject::null())?;

        for (i, &pid) in proc_list.iter().enumerate() {
            let proc_info = driver
                .get_process_info(pid)
                .map_err(|_| anyhow!("Unable to get process info for pid {}", pid))?;

            let proc_info_obj = conversions::proc_info_to_jobject(&mut env, &proc_info)?;
            env.set_object_array_element(&result_array, i as jsize, proc_info_obj)?;
        }

        Ok(result_array)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeBindProcess", "(I)Z")]
pub fn jni_bind_proc(mut env: JNIEnv, _obj: JObject, pid: jint) -> jboolean {
    (|| -> JniResult<jboolean> {
        let manager_read = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;
        let driver = manager_read.get_driver()
            .ok_or_else(|| anyhow!("Driver is not initialized"))?;

        let Ok(bind_proc) = driver.bind_process(pid) else {
            return Ok(JNI_FALSE);
        };
        drop(manager_read);

        let mut manager_write = DRIVER_MANAGER.write()
            .map_err(|_| anyhow!("Failed to acquire DriverManager write lock"))?;
        manager_write.bind_process(bind_proc, pid)?;

        debug!("{}: {}", s!("绑定进程成功，PID"), pid);
        Ok(JNI_TRUE)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeGetCurrentBindPid", "()I")]
pub fn jni_get_current_bind_pid(_env: JNIEnv, _obj: JObject) -> jint {
    if let Ok(manager) = DRIVER_MANAGER.read() {
        manager.get_bound_pid()
    } else {
        0
    }
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeIsProcessBound", "()Z")]
pub fn jni_is_proc_bound(_env: JNIEnv, _obj: JObject) -> jboolean {
    if let Ok(manager) = DRIVER_MANAGER.read() {
        if manager.is_process_bound() {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    } else {
        JNI_FALSE
    }
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeUnbindProcess", "()Z")]
pub fn jni_unbind_proc(mut env: JNIEnv, _obj: JObject) -> jboolean {
    (|| -> JniResult<jboolean> {
        let mut manager = DRIVER_MANAGER.write()
            .map_err(|_| anyhow!("Failed to acquire DriverManager write lock"))?;
        manager.unbind_process();
        debug!("{}", s!("释放进程绑定成功"));
        Ok(JNI_TRUE)
    })()
    .or_throw(&mut env)
}

#[jni_method(80, "moe/fuqiuluo/mamu/driver/WuwaDriver", "nativeQueryMemRegions", "(I)[Lmoe/fuqiuluo/mamu/driver/MemRegionEntry;")]
pub fn jni_query_mem_regions<'l>(
    mut env: JNIEnv<'l>,
    _obj: JObject,
    pid: jint,
) -> JObjectArray<'l> {
    use std::os::fd::BorrowedFd;

    (|| -> JniResult<JObjectArray<'l>> {
        let manager = DRIVER_MANAGER.read()
            .map_err(|_| anyhow!("Failed to acquire DriverManager read lock"))?;

        if !manager.is_process_bound() {
            return Err(anyhow!("No process is bound. Please bind a process before querying memory regions."));
        }

        let driver = manager.get_driver()
            .ok_or_else(|| anyhow!("Driver is not initialized"))?;

        let result = driver
            .query_mem_regions(pid, 0, 0)
            .map_err(|e| anyhow!("Unable to get memory regions for pid {}: {}", pid, e))?;

        info!(
            "Query memory regions: fd={}, buffer_size={}, entry_count={}",
            result.fd, result.buffer_size, result.entry_count
        );

        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(result.fd) };

        let mapped = unsafe {
            mmap(
                None,
                NonZeroUsize::new(result.buffer_size).ok_or_else(|| anyhow!("Invalid buffer size"))?,
                ProtFlags::PROT_READ,
                MapFlags::MAP_PRIVATE,
                borrowed_fd,
                0,
            )
        };

        let mapped_ptr = match mapped {
            Ok(ptr) => ptr,
            Err(e) => {
                unsafe { close(result.fd) };
                return Err(anyhow!("Failed to mmap memory regions buffer: {}", e));
            },
        };

        let entries = mapped_ptr.as_ptr() as *const WuwaMemRegionEntry;

        // 收集过滤后的内存区域
        let mut filtered_entries = Vec::new();
        for i in 0..result.entry_count {
            let entry = unsafe { &*entries.add(i) };
            filtered_entries.push(entry);
        }

        let mem_region_class = env.find_class("moe/fuqiuluo/mamu/driver/MemRegionEntry")?;

        let result_array = env.new_object_array(filtered_entries.len() as jsize, &mem_region_class, JObject::null());

        let result_array = match result_array {
            Ok(arr) => arr,
            Err(e) => {
                unsafe {
                    let _ = munmap(mapped_ptr, result.buffer_size);
                    close(result.fd);
                };
                return Err(anyhow!("Failed to create MemRegionEntry array: {}", e));
            },
        };

        for (i, entry) in filtered_entries.iter().enumerate() {
            match conversions::mem_region_to_jobject(&mut env, entry, &mem_region_class) {
                Ok(entry_obj) => {
                    if let Err(e) = env.set_object_array_element(&result_array, i as jsize, entry_obj) {
                        error!("Failed to set array element at index {}: {}", i, e);
                    }
                },
                Err(e) => {
                    error!("Failed to create MemRegionEntry object at index {}: {}", i, e);
                },
            }
        }

        unsafe {
            let _ = munmap(mapped_ptr, result.buffer_size);
            close(result.fd);
        }

        debug!("Successfully returned {} memory regions (filtered from {})", filtered_entries.len(), result.entry_count);

        Ok(result_array)
    })()
    .or_throw(&mut env)
}