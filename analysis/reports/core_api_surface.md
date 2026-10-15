# Core Modules — Public API Surface

Observable facts (PE export names, import DLLs, behavioral API markers).
We DO NOT reproduce decompiled code or proprietary strings.

## `DRW.exe` (127,208 bytes)

- **Exports**: 0
- **Import DLLs**: 14
- **Import functions (total)**: 130

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`
- **registry**: `RegOpenKey`, `RegQueryValue`
- **process**: `CreateProcess`
- **elevate**: `OpenProcessToken`

---

## `DRWUI.exe` (30,571,752 bytes)

- **Exports**: 9
- **Import DLLs**: 49
- **Import functions (total)**: 3610

### Exports (public API)

- `??0IObject@@QEAA@$$QEAU0@@Z`
- `??0IObject@@QEAA@AEBU0@@Z`
- `??0IObject@@QEAA@XZ`
- `??4IObject@@QEAAAEAU0@$$QEAU0@@Z`
- `??4IObject@@QEAAAEAU0@AEBU0@@Z`
- `??_7IObject@@6B@`
- `?AddRef@IObject@@QEBAKXZ`
- `?GetInterfaceEx@IObject@@QEBAPEAU1@PEBD@Z`
- `?Release@IObject@@QEBAKXZ`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`, `DeviceIoControl`
- **fs**: `FindFirstFile`, `FindNextFile`, `GetDiskFreeSpace`, `GetLogicalDrives`, `GetVolumeInformation`
- **memory**: `VirtualAlloc`
- **registry**: `RegCreateKey`, `RegOpenKey`, `RegQueryValue`, `RegSetValue`
- **process**: `CreateProcess`, `ShellExecute`, `WinExec`
- **net**: `InternetOpen`, `WSAStartup`, `connect`, `socket`
- **crypto**: `CertOpenStore`, `CryptAcquireContext`
- **pipe_ipc**: `ConnectNamedPipe`, `CreateNamedPipe`, `CreatePipe`
- **elevate**: `AdjustTokenPrivileges`, `LookupPrivilegeValue`, `OpenProcessToken`

### Import DLLs

- `advapi32.dll` (58 funcs)
- `api-ms-win-crt-convert-l1-1-0.dll` (14 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (14 funcs)
- `api-ms-win-crt-multibyte-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (27 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (47 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (42 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (3 funcs)
- `archive.dll` (17 funcs)
- `bcrypt.dll` (1 funcs)
- `comctl32.dll` (1 funcs)
- `comdlg32.dll` (1 funcs)
- `crypt32.dll` (17 funcs)
- `gdi32.dll` (13 funcs)
- `gdiplus.dll` (27 funcs)
- `iphlpapi.dll` (1 funcs)
- `kernel32.dll` (218 funcs)
- `libcrypto-1_1-x64.dll` (5 funcs)
- `libeay32.dll` (46 funcs)
- `libzmq-mt-4_3_5.dll` (10 funcs)
- `msi.dll` (5 funcs)
- `msvcp140.dll` (278 funcs)
- `ntdll.dll` (4 funcs)
- `ole32.dll` (14 funcs)
- `oleaut32.dll` (19 funcs)
- `propsys.dll` (2 funcs)
- `publog.dll` (4 funcs)
- `qt5core.dll` (871 funcs)
- `qt5gui.dll` (337 funcs)
- `qt5network.dll` (9 funcs)
- `qt5svg.dll` (9 funcs)
- `qt5widgets.dll` (1246 funcs)
- `qt5winextras.dll` (11 funcs)
- `rpcrt4.dll` (3 funcs)
- `setupapi.dll` (15 funcs)
- `shell32.dll` (20 funcs)
- `shlwapi.dll` (10 funcs)
- `user32.dll` (58 funcs)
- `userenv.dll` (2 funcs)
- `vcruntime140.dll` (23 funcs)
- `version.dll` (3 funcs)
- `wininet.dll` (11 funcs)
- `wintrust.dll` (4 funcs)
- `wldap32.dll` (18 funcs)
- `ws2_32.dll` (36 funcs)

---

## `DRWService.exe` (40,168 bytes)

- **Exports**: 0
- **Import DLLs**: 10
- **Import functions (total)**: 93

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`
- **registry**: `RegOpenKey`, `RegQueryValue`
- **service**: `ControlService`, `CreateService`, `OpenSCManager`, `StartService`
- **driver**: `SCManager`

---

## `DRWReport.exe` (4,805,352 bytes)

- **Exports**: 0
- **Import DLLs**: 18
- **Import functions (total)**: 812

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`
- **fs**: `FindFirstFile`, `FindNextFile`, `GetDiskFreeSpace`, `GetVolumeInformation`
- **memory**: `VirtualAlloc`, `VirtualProtect`
- **registry**: `RegCreateKey`, `RegOpenKey`, `RegQueryValue`, `RegSetValue`
- **process**: `ShellExecute`, `WinExec`
- **net**: `connect`

---

## `NTFSSearchDLL.dll` (577,256 bytes)

- **Exports**: 2
- **Import DLLs**: 13
- **Import functions (total)**: 99

### Exports (public API)

- `EUCreateNTFSQuickSearcher`
- `EUCreateNTFSSearcher`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`, `DeviceIoControl`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (13 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (10 funcs)
- `kernel32.dll` (44 funcs)
- `msvcp140.dll` (5 funcs)
- `publog.dll` (1 funcs)
- `shlwapi.dll` (1 funcs)
- `user32.dll` (1 funcs)
- `vcruntime140.dll` (13 funcs)

---

## `FATSearchDLL.dll` (469,224 bytes)

- **Exports**: 2
- **Import DLLs**: 11
- **Import functions (total)**: 160

### Exports (public API)

- `EUCreateFATQuickSearcher`
- `EUCreateFATSearcher`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`, `DeviceIoControl`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (13 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (17 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (10 funcs)
- `kernel32.dll` (45 funcs)
- `msvcp140.dll` (49 funcs)
- `publog.dll` (1 funcs)
- `shlwapi.dll` (1 funcs)
- `vcruntime140.dll` (16 funcs)

---

## `exFATSearchDLL.dll` (413,416 bytes)

- **Exports**: 2
- **Import DLLs**: 9
- **Import functions (total)**: 163

### Exports (public API)

- `EUCreateEXFATQuickSearcher`
- `EUCreateEXFATSearcher`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (13 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (20 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (15 funcs)
- `kernel32.dll` (34 funcs)
- `msvcp140.dll` (56 funcs)
- `vcruntime140.dll` (17 funcs)

---

## `EXT4SearchDLL.dll` (129,256 bytes)

- **Exports**: 2
- **Import DLLs**: 9
- **Import functions (total)**: 84

### Exports (public API)

- `EUCreateEXT4QuickSearcher`
- `EUCreateEXT4Searcher`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`, `DeviceIoControl`
- **fs**: `FindFirstFile`
- **memory**: `VirtualAlloc`

### Import DLLs

- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (6 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (45 funcs)
- `msvcp140.dll` (3 funcs)
- `shlwapi.dll` (1 funcs)
- `vcruntime140.dll` (12 funcs)

---

## `HFSPlusSearchDLL.dll` (185,064 bytes)

- **Exports**: 2
- **Import DLLs**: 7
- **Import functions (total)**: 81

### Exports (public API)

- `EUCreateHFSQuickSearcher`
- `EUCreateHFSSearcher`

### Import DLLs

- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (30 funcs)
- `msvcp140.dll` (20 funcs)
- `vcruntime140.dll` (14 funcs)

---

## `BtrfsSearchDLL.dll` (150,760 bytes)

- **Exports**: 2
- **Import DLLs**: 8
- **Import functions (total)**: 70

### Exports (public API)

- `EUCreateBtrfsComplateSearcher`
- `EUCreateBtrfsQuickSearcher`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`, `DeviceIoControl`

### Import DLLs

- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (34 funcs)
- `msvcp140.dll` (6 funcs)
- `shlwapi.dll` (1 funcs)
- `vcruntime140.dll` (12 funcs)

---

## `ReFSSearchDLL.dll` (154,856 bytes)

- **Exports**: 2
- **Import DLLs**: 6
- **Import functions (total)**: 49

### Exports (public API)

- `EUCreateReFSQuickSearcher`
- `EUCreateReFSSearcher`

### Import DLLs

- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (2 funcs)
- `kernel32.dll` (16 funcs)
- `msvcp140.dll` (4 funcs)
- `vcruntime140.dll` (13 funcs)

---

## `ZfsSearchDLL.dll` (275,176 bytes)

- **Exports**: 2
- **Import DLLs**: 7
- **Import functions (total)**: 119

### Exports (public API)

- `EUCreateZFSQuickSearcher`
- `EUCreateZFSSearcher`

### Import DLLs

- `api-ms-win-crt-filesystem-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (14 funcs)
- `kernel32.dll` (18 funcs)
- `msvcp140.dll` (56 funcs)
- `vcruntime140.dll` (15 funcs)

---

## `RAWSearchDLL.dll` (644,840 bytes)

- **Exports**: 6
- **Import DLLs**: 14
- **Import functions (total)**: 186

### Exports (public API)

- `EUCreateRAWSearcher`
- `EUCreateRAWSearcherForCellTest`
- `EUGetAnalyserList`
- `EUGetAnalyserMap`
- `EUGetAnalyserMultimap`
- `GetRawUserInfoMap`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-multibyte-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (14 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (18 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (12 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (45 funcs)
- `msvcp140.dll` (64 funcs)
- `publog.dll` (1 funcs)
- `vcruntime140.dll` (18 funcs)
- `ws2_32.dll` (1 funcs)

---

## `PE_Tools.dll` (976,104 bytes)

- **Exports**: 10
- **Import DLLs**: 18
- **Import functions (total)**: 294

### Exports (public API)

- `pet_decompress_packet`
- `pet_download_packet`
- `pet_get_petools_dir`
- `pet_install_driver`
- `pet_is_exist_packet`
- `pet_regist_progress_fun`
- `pet_regist_status_fun`
- `pet_set_download_path`
- `pet_set_progress_data`
- `pet_verify_packet`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`
- **fs**: `FindFirstFile`, `FindNextFile`, `GetDiskFreeSpace`
- **process**: `CreateProcess`
- **net**: `WSAStartup`, `connect`, `socket`
- **crypto**: `CertOpenStore`, `CryptAcquireContext`

### Import DLLs

- `advapi32.dll` (9 funcs)
- `api-ms-win-crt-convert-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (9 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (18 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (30 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (20 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (12 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (1 funcs)
- `bcrypt.dll` (1 funcs)
- `crypt32.dll` (16 funcs)
- `kernel32.dll` (80 funcs)
- `shell32.dll` (1 funcs)
- `vcruntime140.dll` (18 funcs)
- `wldap32.dll` (18 funcs)
- `ws2_32.dll` (35 funcs)

---

## `EUinApp.exe` (37,608 bytes)

- **Exports**: 0
- **Import DLLs**: 5
- **Import functions (total)**: 86

### Behavioral signals (Win32 API usage)

- **registry**: `RegCreateKey`, `RegOpenKey`, `RegQueryValue`, `RegSetValue`

---

## `EuRepairHost.exe` (23,784 bytes)

- **Exports**: 0
- **Import DLLs**: 3
- **Import functions (total)**: 59

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`

---

## `InfoForSetup.exe` (117,776 bytes)

- **Exports**: 0
- **Import DLLs**: 2
- **Import functions (total)**: 71

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`
- **fs**: `FindFirstFile`, `FindNextFile`

---

## `legacy.dll` (101,888 bytes)

- **Exports**: 1
- **Import DLLs**: 4
- **Import functions (total)**: 115

### Exports (public API)

- `OSSL_provider_init`

### Import DLLs

- `api-ms-win-crt-runtime-l1-1-0.dll` (8 funcs)
- `kernel32.dll` (16 funcs)
- `libcrypto-3-x64.dll` (87 funcs)
- `vcruntime140.dll` (4 funcs)

---

## `archive.dll` (688,128 bytes)

- **Exports**: 427
- **Import DLLs**: 15
- **Import functions (total)**: 190

### Exports (public API)

- `archive_bzlib_version`
- `archive_clear_error`
- `archive_compression`
- `archive_compression_name`
- `archive_copy_error`
- `archive_entry_acl`
- `archive_entry_acl_add_entry`
- `archive_entry_acl_add_entry_w`
- `archive_entry_acl_clear`
- `archive_entry_acl_count`
- `archive_entry_acl_from_text`
- `archive_entry_acl_from_text_w`
- `archive_entry_acl_next`
- `archive_entry_acl_reset`
- `archive_entry_acl_text`
- `archive_entry_acl_text_w`
- `archive_entry_acl_to_text`
- `archive_entry_acl_to_text_w`
- `archive_entry_acl_types`
- `archive_entry_atime`
- `archive_entry_atime_is_set`
- `archive_entry_atime_nsec`
- `archive_entry_birthtime`
- `archive_entry_birthtime_is_set`
- `archive_entry_birthtime_nsec`
- `archive_entry_clear`
- `archive_entry_clone`
- `archive_entry_copy_bhfi`
- `archive_entry_copy_fflags_text`
- `archive_entry_copy_fflags_text_w`
- `archive_entry_copy_gname`
- `archive_entry_copy_gname_w`
- `archive_entry_copy_hardlink`
- `archive_entry_copy_hardlink_w`
- `archive_entry_copy_link`
- `archive_entry_copy_link_w`
- `archive_entry_copy_mac_metadata`
- `archive_entry_copy_pathname`
- `archive_entry_copy_pathname_w`
- `archive_entry_copy_sourcepath`
- `archive_entry_copy_sourcepath_w`
- `archive_entry_copy_stat`
- `archive_entry_copy_symlink`
- `archive_entry_copy_symlink_w`
- `archive_entry_copy_uname`
- `archive_entry_copy_uname_w`
- `archive_entry_ctime`
- `archive_entry_ctime_is_set`
- `archive_entry_ctime_nsec`
- `archive_entry_dev`
- `archive_entry_dev_is_set`
- `archive_entry_devmajor`
- `archive_entry_devminor`
- `archive_entry_digest`
- `archive_entry_fflags`
- `archive_entry_fflags_text`
- `archive_entry_filetype`
- `archive_entry_filetype_is_set`
- `archive_entry_free`
- `archive_entry_gid`
- `archive_entry_gid_is_set`
- `archive_entry_gname`
- `archive_entry_gname_utf8`
- `archive_entry_gname_w`
- `archive_entry_hardlink`
- `archive_entry_hardlink_utf8`
- `archive_entry_hardlink_w`
- `archive_entry_ino`
- `archive_entry_ino64`
- `archive_entry_ino_is_set`
- `archive_entry_is_data_encrypted`
- `archive_entry_is_encrypted`
- `archive_entry_is_metadata_encrypted`
- `archive_entry_linkify`
- `archive_entry_linkresolver_free`
- `archive_entry_linkresolver_new`
- `archive_entry_linkresolver_set_strategy`
- `archive_entry_mac_metadata`
- `archive_entry_mode`
- `archive_entry_mtime`
- `archive_entry_mtime_is_set`
- `archive_entry_mtime_nsec`
- `archive_entry_new`
- `archive_entry_new2`
- `archive_entry_nlink`
- `archive_entry_partial_links`
- `archive_entry_pathname`
- `archive_entry_pathname_utf8`
- `archive_entry_pathname_w`
- `archive_entry_perm`
- `archive_entry_perm_is_set`
- `archive_entry_rdev`
- `archive_entry_rdevmajor`
- `archive_entry_rdevminor`
- `archive_entry_set_atime`
- `archive_entry_set_birthtime`
- `archive_entry_set_ctime`
- `archive_entry_set_dev`
- `archive_entry_set_devmajor`
- `archive_entry_set_devminor`
- `archive_entry_set_fflags`
- `archive_entry_set_filetype`
- `archive_entry_set_gid`
- `archive_entry_set_gname`
- `archive_entry_set_gname_utf8`
- `archive_entry_set_hardlink`
- `archive_entry_set_hardlink_utf8`
- `archive_entry_set_ino`
- `archive_entry_set_ino64`
- `archive_entry_set_is_data_encrypted`
- `archive_entry_set_is_metadata_encrypted`
- `archive_entry_set_link`
- `archive_entry_set_link_utf8`
- `archive_entry_set_mode`
- `archive_entry_set_mtime`
- `archive_entry_set_nlink`
- `archive_entry_set_pathname`
- `archive_entry_set_pathname_utf8`
- `archive_entry_set_perm`
- `archive_entry_set_rdev`
- `archive_entry_set_rdevmajor`
- `archive_entry_set_rdevminor`
- `archive_entry_set_size`
- `archive_entry_set_symlink`
- `archive_entry_set_symlink_type`
- `archive_entry_set_symlink_utf8`
- `archive_entry_set_uid`
- `archive_entry_set_uname`
- `archive_entry_set_uname_utf8`
- `archive_entry_size`
- `archive_entry_size_is_set`
- `archive_entry_sourcepath`
- `archive_entry_sourcepath_w`
- `archive_entry_sparse_add_entry`
- `archive_entry_sparse_clear`
- `archive_entry_sparse_count`
- `archive_entry_sparse_next`
- `archive_entry_sparse_reset`
- `archive_entry_stat`
- `archive_entry_strmode`
- `archive_entry_symlink`
- `archive_entry_symlink_type`
- `archive_entry_symlink_utf8`
- `archive_entry_symlink_w`
- `archive_entry_uid`
- `archive_entry_uid_is_set`
- `archive_entry_uname`
- `archive_entry_uname_utf8`
- `archive_entry_uname_w`
- `archive_entry_unset_atime`
- `archive_entry_unset_birthtime`
- `archive_entry_unset_ctime`
- `archive_entry_unset_mtime`
- `archive_entry_unset_size`
- `archive_entry_update_gname_utf8`
- `archive_entry_update_hardlink_utf8`
- `archive_entry_update_link_utf8`
- `archive_entry_update_pathname_utf8`
- `archive_entry_update_symlink_utf8`
- `archive_entry_update_uname_utf8`
- `archive_entry_xattr_add_entry`
- `archive_entry_xattr_clear`
- `archive_entry_xattr_count`
- `archive_entry_xattr_next`
- `archive_entry_xattr_reset`
- `archive_errno`
- `archive_error_string`
- `archive_file_count`
- `archive_filter_bytes`
- `archive_filter_code`
- `archive_filter_count`
- `archive_filter_name`
- `archive_format`
- `archive_format_name`
- `archive_free`
- `archive_liblz4_version`
- `archive_liblzma_version`
- `archive_libzstd_version`
- `archive_match_exclude_entry`
- `archive_match_exclude_pattern`
- `archive_match_exclude_pattern_from_file`
- `archive_match_exclude_pattern_from_file_w`
- `archive_match_exclude_pattern_w`
- `archive_match_excluded`
- `archive_match_free`
- `archive_match_include_date`
- `archive_match_include_date_w`
- `archive_match_include_file_time`
- `archive_match_include_file_time_w`
- `archive_match_include_gid`
- `archive_match_include_gname`
- `archive_match_include_gname_w`
- `archive_match_include_pattern`
- `archive_match_include_pattern_from_file`
- `archive_match_include_pattern_from_file_w`
- `archive_match_include_pattern_w`
- `archive_match_include_time`
- `archive_match_include_uid`
- `archive_match_include_uname`
- `archive_match_include_uname_w`
- ... and 227 more (see inventory.json)

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`, `DeviceIoControl`
- **fs**: `FindFirstFile`, `FindNextFile`, `GetDiskFreeSpace`
- **memory**: `VirtualAlloc`
- **process**: `CreateProcess`
- **crypto**: `BCryptOpen`
- **pipe_ipc**: `CreatePipe`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-environment-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (11 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (15 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (17 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (2 funcs)
- `bcrypt.dll` (13 funcs)
- `kernel32.dll` (70 funcs)
- `liblzma.dll` (14 funcs)
- `vcruntime140.dll` (12 funcs)
- `zlib.dll` (12 funcs)

---

## `cloud_session.dll` (943,336 bytes)

- **Exports**: 4
- **Import DLLs**: 14
- **Import functions (total)**: 290

### Exports (public API)

- `create_con`
- `getLastError`
- `readLastError`
- `recordCloudError`

### Behavioral signals (Win32 API usage)

- **fs**: `FindFirstFile`, `FindNextFile`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (13 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (15 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (3 funcs)
- `concrt140.dll` (1 funcs)
- `cpprest141_2_10.dll` (81 funcs)
- `kernel32.dll` (29 funcs)
- `libcrypto-1_1-x64.dll` (5 funcs)
- `msvcp140.dll` (123 funcs)
- `vcruntime140.dll` (12 funcs)
- `winhttp.dll` (1 funcs)

---

## `AliyunWrap.dll` (624,144 bytes)

- **Exports**: 18
- **Import DLLs**: 21
- **Import functions (total)**: 281

### Exports (public API)

- `?SendLogToCloud@@YA_NAEBV?$vector@U?$pair@PEA_WPEA_W@std@@V?$allocator@U?$pair@PEA_WPEA_W@std@@@2@@std@@@Z`
- `?SendLogToCloud@@YA_NHPEAPEADPEA_K01@Z`
- `AliyunAddParamToSessionA`
- `AliyunAddParamToSessionW`
- `AliyunCloseSession`
- `AliyunEnableUserInfoCollect`
- `AliyunEndCache`
- `AliyunGetKeyValue`
- `AliyunGetUserUid`
- `AliyunInstallConfigFilePath`
- `AliyunInstallInitUid`
- `AliyunIsEnableUserInfoCollect`
- `AliyunOpenSession`
- `AliyunSendInfo`
- `AliyunSetKeyValue`
- `AliyunStopProcess`
- `AliyunUninstallEnd`
- `AliyunUninstallStart`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`
- **registry**: `RegOpenKey`, `RegQueryValue`
- **process**: `CreateProcess`
- **net**: `WSAStartup`, `connect`, `socket`
- **crypto**: `CertOpenStore`, `CryptAcquireContext`
- **elevate**: `AdjustTokenPrivileges`, `LookupPrivilegeValue`, `OpenProcessToken`

### Import DLLs

- `advapi32.dll` (31 funcs)
- `api-ms-win-crt-convert-l1-1-0.dll` (7 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (15 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (24 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (20 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (1 funcs)
- `bcrypt.dll` (1 funcs)
- `crypt32.dll` (16 funcs)
- `iphlpapi.dll` (1 funcs)
- `kernel32.dll` (76 funcs)
- `psapi.dll` (2 funcs)
- `rpcrt4.dll` (1 funcs)
- `shlwapi.dll` (1 funcs)
- `vcruntime140.dll` (17 funcs)
- `wldap32.dll` (18 funcs)
- `ws2_32.dll` (34 funcs)

---

## `EuICloudRecovery.dll` (7,187,688 bytes)

- **Exports**: 7064
- **Import DLLs**: 21
- **Import functions (total)**: 418

### Exports (public API)

- `?$TSS0@?1??get_instance@Logger@eslog@@SAAEAV23@XZ@4HA`
- `??$<move>@AEAU?$pair@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@@YA$$QEAU?$pair@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@AEAU01@@Z`
- `??$<move>@AEAU?$pair@W4EsICloudDataType@@UICloudFileInfo@esicloud@@@std@@@@YA$$QEAU?$pair@W4EsICloudDataType@@UICloudFileInfo@esicloud@@@std@@AEAU01@@Z`
- `??$<move>@AEAU?$pair@_NH@std@@@@YA$$QEAU?$pair@_NH@std@@AEAU01@@Z`
- `??$?0$$BY08D$0A@@path@filesystem@std@@QEAA@AEAY08$$CBDW4format@012@@Z`
- `??$?0$$BY0BC@D$0A@@path@filesystem@std@@QEAA@AEAY0BC@$$CBDW4format@012@@Z`
- `??$?0$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@1@$0A@@?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@QEAA@AEBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@AEBVany@1@@Z`
- `??$?0$$QEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$$Z$$V@?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@QEAA@Upiecewise_construct_t@1@V?$tuple@$$QEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@1@V?$tuple@$$V@1@@Z`
- `??$?0$$QEAW4EsICloudDataType@@$$Z$$V@?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@QEAA@Upiecewise_construct_t@1@V?$tuple@$$QEAW4EsICloudDataType@@@1@V?$tuple@$$V@1@@Z`
- `??$?0$$QEAW4EsIcouldUrlType@esicloud@@$$Z$$V@?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@Upiecewise_construct_t@1@V?$tuple@$$QEAW4EsIcouldUrlType@esicloud@@@1@V?$tuple@$$V@1@@Z`
- `??$?0$$T$$T$0A@@?$pair@P6AHPEAXH@ZPEAX@std@@QEAA@$$QEA$$T0@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@D@std@@V?$_String_val@U?$_Simple_types@D@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@D@std@@V?$_Vector_val@U?$_Simple_types@D@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@E@std@@V?$_Vector_val@U?$_Simple_types@E@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@I@std@@V?$_Vector_val@U?$_Simple_types@I@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@PEAV?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@V?$_Vector_val@U?$_Simple_types@PEAV?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_List_node@UContactPhoneEmail@esicloud@@PEAX@std@@@std@@V?$_List_val@U?$_List_simple_types@UContactPhoneEmail@esicloud@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_List_node@UICloudNoteAttachment@esicloud@@PEAX@std@@@std@@V?$_List_val@U?$_List_simple_types@UICloudNoteAttachment@esicloud@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_List_node@UICloudPhotosZone@IcloudPhotos@esicloud@@PEAX@std@@@std@@V?$_List_val@U?$_List_simple_types@UICloudPhotosZone@IcloudPhotos@esicloud@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$_Tree_node@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U?$pair@HV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@V?$_Vector_val@U?$_Simple_types@U?$pair@HV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@UICloudFileInfo@esicloud@@@std@@V?$_Vector_val@U?$_Simple_types@UICloudFileInfo@esicloud@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U_Grp_t@?$_Tgt_state_t@V?$_String_const_iterator@V?$_String_val@U?$_Simple_types@D@std@@@std@@@std@@@std@@@std@@V?$_Vector_val@U?$_Simple_types@U_Grp_t@?$_Tgt_state_t@V?$_String_const_iterator@V?$_String_val@U?$_Simple_types@D@std@@@std@@@std@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@U_Loop_vals_t@std@@@std@@V?$_Vector_val@U?$_Simple_types@U_Loop_vals_t@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@V?$_Vector_val@U?$_Simple_types@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohm`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Vector_val@U?$_Simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@V?$sub_match@V?$_String_const_iterator@V?$_String_val@U?$_Simple_types@D@std@@@std@@@std@@@std@@@std@@V?$_Vector_val@U?$_Simple_types@V?$sub_match@V?$_String_const_iterator@V?$_String_val@U?$_Simple_types@D@std@@@std@@@std@@@std@@@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Compressed_pair@V?$allocator@_W@std@@V?$_String_val@U?$_Simple_types@_W@std@@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@@Z`
- `??$?0$$V@?$_Ref_count_obj@V?$map@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@V?$allocator@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@2@@std@@@std@@QEAA@XZ`
- `??$?0$$V@?$_Ref_count_obj@V?$map@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@V?$allocator@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@2@@std@@@std@@QEAA@XZ`
- `??$?0$$V@?$_Ref_count_obj@VIcloudClient@esicloud@@@std@@QEAA@XZ`
- `??$?0AEAHAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@HV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAHAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAHH$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAH@Z`
- `??$?0AEANN$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAN@Z`
- `??$?0AEAPEADAEAUICloudFileInfo@esicloud@@$0A@@?$pair@PEADUICloudFileInfo@esicloud@@@std@@QEAA@AEAPEADAEAUICloudFileInfo@esicloud@@@Z`
- `??$?0AEAPEADPEAD$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAPEAD@Z`
- `??$?0AEAPEAV?$tuple@P8ICloudDriveUploadTask@esicloud@@EAAXXZPEAV12@@std@@@?$_Compressed_pair@U?$default_delete@V?$tuple@P8ICloudDriveUploadTask@esicloud@@EAAXXZPEAV12@@std@@@std@@PEAV?$tuple@P8ICloudDriveUploadTask@esicloud@@EAAXXZPEAV12@@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAV?$tuple@P8ICloudDriveUploadTask@esicloud@@EAAXXZPEAV12@@1@@Z`
- `??$?0AEAPEAVIcloudContacts@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudContacts@esicloud@@@std@@PEAVIcloudContacts@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudContacts@esicloud@@@Z`
- `??$?0AEAPEAVIcloudDocuments@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudDocuments@esicloud@@@std@@PEAVIcloudDocuments@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudDocuments@esicloud@@@Z`
- `??$?0AEAPEAVIcloudDrive@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudDrive@esicloud@@@std@@PEAVIcloudDrive@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudDrive@esicloud@@@Z`
- `??$?0AEAPEAVIcloudHealth@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudHealth@esicloud@@@std@@PEAVIcloudHealth@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudHealth@esicloud@@@Z`
- `??$?0AEAPEAVIcloudHomeKitSecureVideo@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudHomeKitSecureVideo@esicloud@@@std@@PEAVIcloudHomeKitSecureVideo@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudHomeKitSecureVideo@esicloud@@@Z`
- `??$?0AEAPEAVIcloudKeychain@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudKeychain@esicloud@@@std@@PEAVIcloudKeychain@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudKeychain@esicloud@@@Z`
- `??$?0AEAPEAVIcloudMail@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudMail@esicloud@@@std@@PEAVIcloudMail@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudMail@esicloud@@@Z`
- `??$?0AEAPEAVIcloudMessages@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudMessages@esicloud@@@std@@PEAVIcloudMessages@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudMessages@esicloud@@@Z`
- `??$?0AEAPEAVIcloudNotes@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudNotes@esicloud@@@std@@PEAVIcloudNotes@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudNotes@esicloud@@@Z`
- `??$?0AEAPEAVIcloudPhotos@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudPhotos@esicloud@@@std@@PEAVIcloudPhotos@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudPhotos@esicloud@@@Z`
- `??$?0AEAPEAVIcloudReminders@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudReminders@esicloud@@@std@@PEAVIcloudReminders@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudReminders@esicloud@@@Z`
- `??$?0AEAPEAVIcloudSafari@esicloud@@@?$_Compressed_pair@U?$default_delete@VIcloudSafari@esicloud@@@std@@PEAVIcloudSafari@esicloud@@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAVIcloudSafari@esicloud@@@Z`
- `??$?0AEAPEAV_Facet_base@std@@@?$_Compressed_pair@U?$default_delete@V_Facet_base@std@@@std@@PEAV_Facet_base@2@$00@std@@QEAA@U_Zero_then_variadic_args_t@1@AEAPEAV_Facet_base@1@@Z`
- `??$?0AEAU?$equal_to@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@M@?$_Compressed_pair@U?$equal_to@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@M$00@std@@QEAA@U_One_then_variadic_args_t@1@AEAU?$equal_to@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@1@$$QEAM@Z`
- `??$?0AEAU?$hash@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_One_then_variadic_args_t@1@AEAU?$equal_to@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@1@M@?$_Compressed_pair@U?$hash@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@U?$equal_to@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@M$00@2@$00@std@@QEAA@U_One_then_variadic_args_t@1@AEAU?$hash@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@`
- `??$?0AEAV?$_Tree_const_iterator@V?$_Tree_val@U?$_Tree_simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_const_iterator@V?$_Tree_val@U?$_Tree_simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_const_iterator@V?$_Tree_val@U?$_Tree_simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@1@$$QEA_N@Z`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@std@@@std@@@1@$$QEA_N@Z`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@std@@@std@@@1@$$Q`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsICloudDataTyp`
- `??$?0AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@std@@@std@@_N$0A@@?$pair@V?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@std@@@std@@_N@std@@QEAA@AEAV?$_Tree_iterator@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$cha`
- `??$?0AEAV?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAV?$basi`
- `??$?0AEAV?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@$0A@@any@std@@QEAA@AEAV?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@Z`
- `??$?0AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@Z`
- `??$?0AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Ref_count_obj@V?$output_string_adapter@DV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@detail@json_abi_v3_12_0@nlohmann@@@std@@QEAA@AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V01@$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@Z`
- `??$?0AEAV?$vector@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$allocator@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@@std@@$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAV?$vector@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std`
- `??$?0AEAV?$vector@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$allocator@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@@std@@V01@$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAV?$vector@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$allocator@V?$basic_string@DU?$char_tra`
- `??$?0AEAY00$$CBD$$BY00D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY00$$CBD@Z`
- `??$?0AEAY02$$CBD$$BY02D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY02$$CBD@Z`
- `??$?0AEAY02$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY02$$CBD@Z`
- `??$?0AEAY03$$CBD$$BY03D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY03$$CBD@Z`
- `??$?0AEAY04$$CBD$$BY04D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY04$$CBD@Z`
- `??$?0AEAY04$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY04$$CBD@Z`
- `??$?0AEAY05$$CBD$$BY05D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY05$$CBD@Z`
- `??$?0AEAY05$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY05$$CBD@Z`
- `??$?0AEAY06$$CBD$$BY06D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY06$$CBD@Z`
- `??$?0AEAY06$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY06$$CBD@Z`
- `??$?0AEAY06$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@PEBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAY06$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAY07$$CBD$$BY07D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY07$$CBD@Z`
- `??$?0AEAY07$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY07$$CBD@Z`
- `??$?0AEAY07$$CBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@PEBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAY07$$CBD$$QEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAY08$$CBD$$BY08D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY08$$CBD@Z`
- `??$?0AEAY08$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY08$$CBD@Z`
- `??$?0AEAY08$$CBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@PEBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAY08$$CBD$$QEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAY09$$CBD$$BY09D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY09$$CBD@Z`
- `??$?0AEAY09$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY09$$CBD@Z`
- `??$?0AEAY0BA@$$CBD$$BY0BA@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BA@$$CBD@Z`
- `??$?0AEAY0BA@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BA@$$CBD@Z`
- `??$?0AEAY0BB@$$CBD$$BY0BB@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BB@$$CBD@Z`
- `??$?0AEAY0BB@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BB@$$CBD@Z`
- `??$?0AEAY0BC@$$CBD$$BY0BC@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BC@$$CBD@Z`
- `??$?0AEAY0BC@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BC@$$CBD@Z`
- `??$?0AEAY0BD@$$CBD$$BY0BD@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BD@$$CBD@Z`
- `??$?0AEAY0BD@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BD@$$CBD@Z`
- `??$?0AEAY0BE@$$CBD$$BY0BE@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BE@$$CBD@Z`
- `??$?0AEAY0BE@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BE@$$CBD@Z`
- `??$?0AEAY0BE@$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@PEBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAY0BE@$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAY0BF@$$CBD$$BY0BF@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BF@$$CBD@Z`
- `??$?0AEAY0BF@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BF@$$CBD@Z`
- `??$?0AEAY0BG@$$CBD$$BY0BG@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BG@$$CBD@Z`
- `??$?0AEAY0BG@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BG@$$CBD@Z`
- `??$?0AEAY0BH@$$CBD$$BY0BH@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BH@$$CBD@Z`
- `??$?0AEAY0BH@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BH@$$CBD@Z`
- `??$?0AEAY0BI@$$CBD$$BY0BI@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BI@$$CBD@Z`
- `??$?0AEAY0BJ@$$CBD$$BY0BJ@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BJ@$$CBD@Z`
- `??$?0AEAY0BJ@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BJ@$$CBD@Z`
- `??$?0AEAY0BL@$$CBD$$BY0BL@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BL@$$CBD@Z`
- `??$?0AEAY0BL@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BL@$$CBD@Z`
- `??$?0AEAY0BL@$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@?$pair@PEBDV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@QEAA@AEAY0BL@$$CBDAEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEAY0BM@$$CBD$$BY0BM@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BM@$$CBD@Z`
- `??$?0AEAY0BM@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BM@$$CBD@Z`
- `??$?0AEAY0BN@$$CBD$$BY0BN@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BN@$$CBD@Z`
- `??$?0AEAY0BN@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BN@$$CBD@Z`
- `??$?0AEAY0BO@$$CBD$$BY0BO@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BO@$$CBD@Z`
- `??$?0AEAY0BO@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0BO@$$CBD@Z`
- `??$?0AEAY0CA@$$CBD$$BY0CA@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CA@$$CBD@Z`
- `??$?0AEAY0CA@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CA@$$CBD@Z`
- `??$?0AEAY0CC@$$CBD$$BY0CC@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CC@$$CBD@Z`
- `??$?0AEAY0CD@$$CBD$$BY0CD@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CD@$$CBD@Z`
- `??$?0AEAY0CD@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CD@$$CBD@Z`
- `??$?0AEAY0CE@$$CBD$$BY0CE@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CE@$$CBD@Z`
- `??$?0AEAY0CE@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CE@$$CBD@Z`
- `??$?0AEAY0CF@$$CBD$$BY0CF@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CF@$$CBD@Z`
- `??$?0AEAY0CF@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CF@$$CBD@Z`
- `??$?0AEAY0CK@$$CBD$$BY0CK@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CK@$$CBD@Z`
- `??$?0AEAY0CK@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0CK@$$CBD@Z`
- `??$?0AEAY0DB@$$CBD$$BY0DB@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0DB@$$CBD@Z`
- `??$?0AEAY0L@$$CBD$$BY0L@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0L@$$CBD@Z`
- `??$?0AEAY0L@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0L@$$CBD@Z`
- `??$?0AEAY0M@$$CBD$$BY0M@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0M@$$CBD@Z`
- `??$?0AEAY0M@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0M@$$CBD@Z`
- `??$?0AEAY0N@$$CBD$$BY0N@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0N@$$CBD@Z`
- `??$?0AEAY0N@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0N@$$CBD@Z`
- `??$?0AEAY0O@$$CBD$$BY0O@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0O@$$CBD@Z`
- `??$?0AEAY0O@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0O@$$CBD@Z`
- `??$?0AEAY0P@$$CBD$$BY0P@D$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0P@$$CBD@Z`
- `??$?0AEAY0P@$$CBD$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEAY0P@$$CBD@Z`
- `??$?0AEA_J$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEA_J@Z`
- `??$?0AEA_J_J$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEA_J@Z`
- `??$?0AEA_K_K$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEA_K@Z`
- `??$?0AEA_N$0A@@?$json_ref@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@detail@json_abi_v3_12_0@nlohmann@@QEAA@AEA_N@Z`
- `??$?0AEA_N_N$0A@@?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@QEAA@AEA_N@Z`
- `??$?0AEBU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_One_then_variadic_args_t@1@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@PEAX@std@@@1@@?$_Compressed_pair@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_typ`
- `??$?0AEBU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@UICloudFileInfo@esicloud`
- `??$?0AEBU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V12@@std@@@std@@@2@$00@2@$00@std@@QEAA@U_One_t`
- `??$?0AEBU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@Vany@2@@std@@@std@@@2@$00@2@$00@std@@QEAA@U`
- `??$?0AEBU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@2@$00@2@$00@std@@QEAA@U_One_then_variadic_args_t@1@AEBU?$less@V?$basic_st`
- `??$?0AEBU?$less@W4EsICloudDataType@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@W4EsICloudDataType@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsICloudDataType@@V?$vector@UICloudFileInfo@esicloud@@V?$allocator@UICloudFileInfo@esicloud@@@std@@@std@@@std@@@std@@@2@$00@2@$00@std@@QEAA@U_One_the`
- `??$?0AEBU?$less@W4EsIcouldUrlType@esicloud@@@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@W4EsIcouldUrlType@esicloud@@@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBW4EsIcouldUrlType@esicloud@@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@std@@@std@@@2@$00@2@$00@std@@QEAA@U_One_then_variadic_args_t@`
- `??$?0AEBU?$less@X@std@@U_One_then_variadic_args_t@1@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@PEAX@std@@@1@@?$_Compressed_pair@U?$less@X@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_tr`
- `??$?0AEBU?$less@X@std@@U_Zero_then_variadic_args_t@1@@?$_Compressed_pair@U?$less@X@std@@V?$_Compressed_pair@V?$allocator@U?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@@std@@PEAX@std@@@std@@V?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string`
- `??$?0AEBV<lambda_016eae8b7f456bc7c593315cd8e8b92c>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_016eae8b7f456bc7c593315cd8e8b92c>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_016eae8b7f456bc7c593315cd8e8b92c>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_0d2b12eb5d7bf921e674c9c7cc5ac1db>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_0d2b12eb5d7bf921e674c9c7cc5ac1db>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_0d2b12eb5d7bf921e674c9c7cc5ac1db>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_1bf0d253d1bb0bc46bfb5ff2d40ba563>@@AEAPEAV?$vector@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@V?$allocator@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlo`
- `??$?0AEBV<lambda_1bf6a290f16c993ca7ece77bd6b26f00>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_1bf6a290f16c993ca7ece77bd6b26f00>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_1bf6a290f16c993ca7ece77bd6b26f00>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_1ee206d572ca61993e8c1bcf5a11cd3f>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_1ee206d572ca61993e8c1bcf5a11cd3f>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_1ee206d572ca61993e8c1bcf5a11cd3f>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_2479a33b95f7d016e9efed2958c975c7>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_2479a33b95f7d016e9efed2958c975c7>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_2479a33b95f7d016e9efed2958c975c7>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_2cb4dc375079b2e7c8c8603d7eef3fee>@@AEAPEAV?$vector@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@V?$allocator@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlo`
- `??$?0AEBV<lambda_31a0db4b4d17f6342bba1eca7ac28f79>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_31a0db4b4d17f6342bba1eca7ac28f79>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_31a0db4b4d17f6342bba1eca7ac28f79>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_32546bbc0069cdc87760603c3f320b4b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_32546bbc0069cdc87760603c3f320b4b>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_32546bbc0069cdc87760603c3f320b4b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_3b7839927b980adba90b758af728368d>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_3b7839927b980adba90b758af728368d>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_3b7839927b980adba90b758af728368d>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_3e56097e1a048f8aa393689321c436c8>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_3e56097e1a048f8aa393689321c436c8>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_3e56097e1a048f8aa393689321c436c8>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_3efc25f2760df354eb63cf0e58d3db6d>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_3efc25f2760df354eb63cf0e58d3db6d>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_3efc25f2760df354eb63cf0e58d3db6d>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_40eed5987ea12d1ad76e91c6ac66d5bc>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_40eed5987ea12d1ad76e91c6ac66d5bc>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_40eed5987ea12d1ad76e91c6ac66d5bc>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_428b5eda7405c5fbac261ef099be2db8>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_428b5eda7405c5fbac261ef099be2db8>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_428b5eda7405c5fbac261ef099be2db8>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_498feaa0c0e46a59c7753b8f6b998307>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_498feaa0c0e46a59c7753b8f6b998307>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_498feaa0c0e46a59c7753b8f6b998307>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_5a0cf873be275c3c56ae22da03882371>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_5a0cf873be275c3c56ae22da03882371>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_5a0cf873be275c3c56ae22da03882371>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_7174ce4e7f549b4f4900f297ff42f9bb>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_7174ce4e7f549b4f4900f297ff42f9bb>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_7174ce4e7f549b4f4900f297ff42f9bb>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_7487684b661df03b843381cd7721abdf>@@AEAPEAV?$map@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@U?$less@X@2@V?$allocator@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D`
- `??$?0AEBV<lambda_77e93b43a6d0b0151ee5e69b5fb8e8ca>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_77e93b43a6d0b0151ee5e69b5fb8e8ca>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_77e93b43a6d0b0151ee5e69b5fb8e8ca>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_781b1b5b2711ed755c5b71b17f3172af>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_781b1b5b2711ed755c5b71b17f3172af>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_781b1b5b2711ed755c5b71b17f3172af>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_829d1095674799a1f5327387ae1b5739>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_829d1095674799a1f5327387ae1b5739>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_829d1095674799a1f5327387ae1b5739>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_82b03d81c477963f3c6e0e6fe0fc58dc>@@AEAPEAV?$vector@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@V?$allocator@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlo`
- `??$?0AEBV<lambda_85ad2fe07cc44a8cd697b8b7558c357b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_85ad2fe07cc44a8cd697b8b7558c357b>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_85ad2fe07cc44a8cd697b8b7558c357b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_872ef15a6b8a7d35c590df3fab77e9fa>@@AEAPEAV?$vector@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlohmann@@V?$allocator@V?$basic_json@Vmap@std@@Vvector@2@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@_N_J_KNVallocator@2@Uadl_serializer@json_abi_v3_12_0@nlohmann@@V?$vector@EV?$allocator@E@std@@@2@X@json_abi_v3_12_0@nlo`
- `??$?0AEBV<lambda_89bcb9f7b3925181784727b2818ee257>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_89bcb9f7b3925181784727b2818ee257>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_89bcb9f7b3925181784727b2818ee257>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_9e8384b45559f29bf9a86f046ed34c5e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_9e8384b45559f29bf9a86f046ed34c5e>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_9e8384b45559f29bf9a86f046ed34c5e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_a1f1ffe590139249457c5595b2625bda>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_a1f1ffe590139249457c5595b2625bda>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_a1f1ffe590139249457c5595b2625bda>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_aa411a356adad4fdcb9e7578376e490e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_aa411a356adad4fdcb9e7578376e490e>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_aa411a356adad4fdcb9e7578376e490e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_abd5e4e71925abeb6b28c28d1f89374b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_abd5e4e71925abeb6b28c28d1f89374b>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_abd5e4e71925abeb6b28c28d1f89374b>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_b753403497ca21e424d070b02dbf9880>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_b753403497ca21e424d070b02dbf9880>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_b753403497ca21e424d070b02dbf9880>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_bca33ab28a36563d9d82a469b09a1d50>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_bca33ab28a36563d9d82a469b09a1d50>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_bca33ab28a36563d9d82a469b09a1d50>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_c291efe09bf205fff6223231034dd73e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_c291efe09bf205fff6223231034dd73e>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_c291efe09bf205fff6223231034dd73e>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_ce3a79994481885624b0d15b366bcf83>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_ce3a79994481885624b0d15b366bcf83>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_ce3a79994481885624b0d15b366bcf83>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_db3ff88b08a926af6f089ffe64386564>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_db3ff88b08a926af6f089ffe64386564>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_db3ff88b08a926af6f089ffe64386564>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_dcfb9a7518ecdce1aef7ebc05a6befdc>@@AEAPEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@@?$_Compressed_pair@V<lambda_dcfb9a7518ecdce1aef7ebc05a6befdc>@@PEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_dcfb9a7518ecdce1aef7ebc05a6befdc>@@AEAPEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@@Z`
- `??$?0AEBV<lambda_df3fce0356cba879246c6c64d8d8dea1>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_df3fce0356cba879246c6c64d8d8dea1>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_df3fce0356cba879246c6c64d8d8dea1>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_dfa7ec2bf7d7b1e368c3985d8d3e3585>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_dfa7ec2bf7d7b1e368c3985d8d3e3585>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_dfa7ec2bf7d7b1e368c3985d8d3e3585>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_e6f8f557c9dd2954a284c446412a21c5>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_e6f8f557c9dd2954a284c446412a21c5>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_e6f8f557c9dd2954a284c446412a21c5>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_e809640c54a4ab2a17dad9f8891eaab2>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_e809640c54a4ab2a17dad9f8891eaab2>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_e809640c54a4ab2a17dad9f8891eaab2>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_ee421c7137c4977721e92c41c0d37c61>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_ee421c7137c4977721e92c41c0d37c61>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_ee421c7137c4977721e92c41c0d37c61>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_f39136eec920075880acb8c667a8f792>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_f39136eec920075880acb8c667a8f792>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_f39136eec920075880acb8c667a8f792>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_f41bba18f0830ddacd781f55fe00d453>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_f41bba18f0830ddacd781f55fe00d453>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_f41bba18f0830ddacd781f55fe00d453>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_f8ba75b1b2c8f06c5b1b0bf4c92c49d6>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@?$_Compressed_pair@V<lambda_f8ba75b1b2c8f06c5b1b0bf4c92c49d6>@@PEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_f8ba75b1b2c8f06c5b1b0bf4c92c49d6>@@AEAPEAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@@Z`
- `??$?0AEBV<lambda_f941a980ce1a646c97a2155b02cf038f>@@AEAPEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@@?$_Compressed_pair@V<lambda_f941a980ce1a646c97a2155b02cf038f>@@PEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@$0A@@std@@QEAA@U_One_then_variadic_args_t@1@AEBV<lambda_f941a980ce1a646c97a2155b02cf038f>@@AEAPEAV?$byte_container_with_subtype@V?$vector@EV?$allocator@E@std@@@std@@@json_abi_v3_12_0@nlohmann@@@Z`
- ... and 6864 more (see inventory.json)

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`
- **fs**: `FindFirstFile`, `FindNextFile`
- **net**: `WSAStartup`, `connect`, `socket`
- **crypto**: `CertOpenStore`, `CryptAcquireContext`, `CryptDecrypt`

### Import DLLs

- `advapi32.dll` (19 funcs)
- `api-ms-win-crt-convert-l1-1-0.dll` (8 funcs)
- `api-ms-win-crt-environment-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (6 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-multibyte-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (17 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (31 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (20 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (6 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (1 funcs)
- `bcrypt.dll` (1 funcs)
- `crypt32.dll` (10 funcs)
- `iphlpapi.dll` (1 funcs)
- `kernel32.dll` (91 funcs)
- `msvcp140.dll` (136 funcs)
- `user32.dll` (3 funcs)
- `vcruntime140.dll` (18 funcs)
- `ws2_32.dll` (38 funcs)

---

## `firebaseonlineconfig.dll` (1,155,816 bytes)

- **Exports**: 2
- **Import DLLs**: 19
- **Import functions (total)**: 333

### Exports (public API)

- `create_config`
- `release_config`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`
- **net**: `WSAStartup`, `WinHttpOpen`, `connect`, `socket`
- **crypto**: `CertOpenStore`, `CryptAcquireContext`

### Import DLLs

- `advapi32.dll` (7 funcs)
- `api-ms-win-crt-convert-l1-1-0.dll` (10 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (6 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (5 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (6 funcs)
- `api-ms-win-crt-multibyte-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (17 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (23 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (16 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-utility-l1-1-0.dll` (2 funcs)
- `crypt32.dll` (15 funcs)
- `kernel32.dll` (53 funcs)
- `msvcp140.dll` (106 funcs)
- `ole32.dll` (1 funcs)
- `vcruntime140.dll` (16 funcs)
- `winhttp.dll` (12 funcs)
- `ws2_32.dll` (28 funcs)

---

## `PubLog.dll` (29,928 bytes)

- **Exports**: 14
- **Import DLLs**: 5
- **Import functions (total)**: 54

### Exports (public API)

- `?DebugTrace@@YAXPEBDPEAPEAD@Z`
- `?DebugTrace@@YAXPEBDZZ`
- `?FileNameToFileCode@@YAGPEAD@Z`
- `?InitLog@@YAXPEBDW4LOGLEVEL@@W4LOGTYPE@@@Z`
- `?Log@@YAXW4LOGLEVEL@@PEBDPEAPEAD@Z`
- `?Log@@YAXW4LOGLEVEL@@PEBDZZ`
- `?LogError@@YAXPEBDPEAPEAD@Z`
- `?LogError@@YAXPEBDZZ`
- `?LogInfo@@YAXPEBDPEAPEAD@Z`
- `?LogInfo@@YAXPEBDZZ`
- `?LogWarning@@YAXPEBDPEAPEAD@Z`
- `?LogWarning@@YAXPEBDZZ`
- `?ReleaseTrace@@YAXPEBDPEAPEAD@Z`
- `?ReleaseTrace@@YAXPEBDZZ`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`

### Import DLLs

- `api-ms-win-crt-heap-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (10 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (34 funcs)
- `vcruntime140.dll` (8 funcs)

---

## `EUHwpFilePathCheckerModule.dll` (155,880 bytes)

- **Exports**: 1
- **Import DLLs**: 2
- **Import functions (total)**: 96

### Exports (public API)

- `IsAccessiblePath`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`
- **memory**: `VirtualAlloc`

### Import DLLs

- `kernel32.dll` (85 funcs)
- `user32.dll` (11 funcs)

---

## `EUVideoRepair.dll` (948,456 bytes)

- **Exports**: 1
- **Import DLLs**: 14
- **Import functions (total)**: 201

### Exports (public API)

- `GetIMovMp4Repair`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileW`, `DeviceIoControl`

### Import DLLs

- `api-ms-win-crt-convert-l1-1-0.dll` (1 funcs)
- `api-ms-win-crt-filesystem-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-heap-l1-1-0.dll` (3 funcs)
- `api-ms-win-crt-locale-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-math-l1-1-0.dll` (2 funcs)
- `api-ms-win-crt-runtime-l1-1-0.dll` (13 funcs)
- `api-ms-win-crt-stdio-l1-1-0.dll` (15 funcs)
- `api-ms-win-crt-string-l1-1-0.dll` (4 funcs)
- `api-ms-win-crt-time-l1-1-0.dll` (1 funcs)
- `kernel32.dll` (41 funcs)
- `msvcp140.dll` (100 funcs)
- `shlwapi.dll` (1 funcs)
- `user32.dll` (1 funcs)
- `vcruntime140.dll` (15 funcs)

---

## `EUImageEnhance.dll` (3,056,872 bytes)

- **Exports**: 4
- **Import DLLs**: 3
- **Import functions (total)**: 142

### Exports (public API)

- `BmpBlur`
- `EUCalcDamageProportion`
- `EUbeSimilarPictures`
- `EnhanceMemData`

### Behavioral signals (Win32 API usage)

- **raw_disk**: `CreateFileA`, `CreateFileW`
- **memory**: `VirtualAlloc`, `VirtualProtect`

### Import DLLs

- `gdiplus.dll` (12 funcs)
- `kernel32.dll` (128 funcs)
- `user32.dll` (2 funcs)

---

## `EuUserRate.dll` (1,619,176 bytes)

- **Exports**: 60
- **Import DLLs**: 12
- **Import functions (total)**: 365

### Exports (public API)

- `EuInitialize`
- `EuShowWindow`
- `EuUnInitialize`
- `curl_easy_cleanup`
- `curl_easy_duphandle`
- `curl_easy_escape`
- `curl_easy_getinfo`
- `curl_easy_init`
- `curl_easy_pause`
- `curl_easy_perform`
- `curl_easy_recv`
- `curl_easy_reset`
- `curl_easy_send`
- `curl_easy_setopt`
- `curl_easy_strerror`
- `curl_easy_unescape`
- `curl_escape`
- `curl_formadd`
- `curl_formfree`
- `curl_formget`
- `curl_free`
- `curl_getdate`
- `curl_getenv`
- `curl_global_cleanup`
- `curl_global_init`
- `curl_global_init_mem`
- `curl_maprintf`
- `curl_mfprintf`
- `curl_mprintf`
- `curl_msnprintf`
- `curl_msprintf`
- `curl_multi_add_handle`
- `curl_multi_assign`
- `curl_multi_cleanup`
- `curl_multi_fdset`
- `curl_multi_info_read`
- `curl_multi_init`
- `curl_multi_perform`
- `curl_multi_remove_handle`
- `curl_multi_setopt`
- `curl_multi_socket`
- `curl_multi_socket_action`
- `curl_multi_socket_all`
- `curl_multi_strerror`
- `curl_multi_timeout`
- `curl_multi_wait`
- `curl_mvaprintf`
- `curl_mvfprintf`
- `curl_mvprintf`
- `curl_mvsnprintf`
- `curl_mvsprintf`
- `curl_share_cleanup`
- `curl_share_init`
- `curl_share_setopt`
- `curl_share_strerror`
- `curl_slist_append`
- `curl_slist_free_all`
- `curl_strequal`
- `curl_strnequal`
- `curl_unescape`

### Behavioral signals (Win32 API usage)

- **fs**: `FindFirstFile`, `FindNextFile`
- **net**: `WSAStartup`, `connect`, `socket`

### Import DLLs

- `advapi32.dll` (3 funcs)
- `gdi32.dll` (12 funcs)
- `gdiplus.dll` (36 funcs)
- `kernel32.dll` (59 funcs)
- `msvcp90.dll` (67 funcs)
- `msvcr90.dll` (121 funcs)
- `ole32.dll` (1 funcs)
- `oleaut32.dll` (1 funcs)
- `sensapi.dll` (1 funcs)
- `shell32.dll` (1 funcs)
- `user32.dll` (37 funcs)
- `ws2_32.dll` (26 funcs)

---

## `Japanese.dll` (4,206,312 bytes)

- **Exports**: 0
- **Import DLLs**: 2
- **Import functions (total)**: 30

---
