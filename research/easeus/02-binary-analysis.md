# EaseUS DRW — Deep Binary RE Findings (DRWUI.exe)

**Binary**: DRWUI.exe, 30.5 MB, PE32+ x64 MSVC, canary+NX+PIE, Qt 5 (core/gui/network/widgets/winextras/svg) + **libzmq-mt-4.3.5** + libarchive (archive.dll) + OpenSSL 1.1 + ATL.
**Build path leak**: `e:\drw\_drw_main\drwintelligentscan\...` (module map below).

## 1. Process & IPC architecture

| Process | Role |
|---|---|
| DRWUI.exe | Qt Widgets wizard UI (all flows) — embeds the scan/recovery controller layer |
| DRW.exe | engine loader (PubLog.dll, version.ini, spawns engine; 127 KB) |
| DRWService.exe | Windows service for elevated raw access (40 KB) |
| EUinApp.exe / EURepairHost.exe / EuDownloader.exe | store / repair host / downloader |

IPC: **ZeroMQ over tcp://localhost:<dynamic port>** (127.0.0.1), message pattern:
- UI receives: `onReceiveData(int msgType, void* payload, int&)` → `doDispatchMessage(...)` per-view dispatch
- Engine → UI **Sink** classes stream incremental updates:
  `CTreeviewSink` (tree), `CListviewSink` (list), `CUISink` (master), `CPEUISink` (WinPE), `IAdvanceFileFilterSink`, `IPreviewSink`, `IWorkerSink`, `IRecoveryController` callbacks
- Preview data channel: separate TCP (`VAPreviewListener::SyncWait4DataBack/RecvThumbnailBmpData` — raw MediaInfo + thumbnail BMP frames)
- Logging: `%s:%d failed got socket / failed send data to peer` (ZMQ), `msg id=%d, msg size=%d`
- Scan commands (CScanMgr): `doScan, doScanCloud, doScanRemoteDevice, doStartLogic, doExit`, completion signal `ScanFinished`
- Scan params surface: `Scan drive:%c, Partition type:%d, strFolder:%s, Scan Desktop:%d, Scan Recyclebin:%d, ---Cloud local scan---, type=%d`
- Live lost-partition discovery: **`sigFoundQuickPartitonNotify`** → `sigRefreshLeftTree`
- Tree live updates: `OnSinkInsertNode/RemoveItem/SelectItem/ExpandPathItem/Refresh/RefreshRoot/DoBeforeFreeze/Unfreeze`
- Progress: `sigProgressUpdated(CProgressInfo)`, `onProgressUpdated`, `sigIsStopping(bool&)`, `sigIsBusy(bool&)`

## 2. Scan pipeline semantics (from strings/logs)

1. **Quick Scan** (`m_iScanType`, "Quick Scan Begin/Finish"): filesystem metadata scan (deleted entries + lost partition discovery) — minutes. → `onQuickScanCompleted(int)` / `sigQuickScanFinished(int)` → `CQuickScanCompleteWnd` (title, tips, content left/right, bottom CTA).
2. **Advanced Scan** ("Advanced Scanning%1, Files Found:", sector-by-sector + signature carving). Runs *after/alongside* quick results; UI stays browsable — "You can preview and recover your files during the scanning process."
3. **Smart Partition Scan** (`Smart Partition Scanning`, "Smart scan in progress, analyzing file chunks information") — lost partition reconstruction mode.
4. States: `Scanning%1, Files Found:` · `Scan Paused%1, Files found:` · `Analyzing %1%` · `Organizing Found Files`/`Organizing found files...` · `Filtering` (post-filter thread) · `complete` / `stopped`.
5. Bad sectors: "Program is trying to skip bad sectors to find data." (BadSectorTipsDlg), disk-image recommendation for failing disks (CreateImage flow, `Disk image is a copy of the entire disk, which can improve the recovery performance`).
6. Auto-Recover: `CAutoRecoverMgr` + AutoRecoverSettingWnd ("Auto-Recover after Advanced Scan Completed").
7. Session: save/load `*.rsf` ("Recovery State File"), `LastScanRecordView`, "Continue last scanning", "Load Scan Session Successful, Scan files:".

## 3. The C-class architecture (RTTI extraction, ~700 classes)

Layered source tree `drw_main\drwintelligentscan\`:
- `qtdrwui\` — Qt UI (detailpage\bottomarea.cpp etc.)
- `recoverycontroller\` — CGeneric/CDeleted/CRecycleBin/CComplete/CRAW/CCloud Search Controllers
- `filerecover\`, `cloudsearch\`, `hfspluslib\`, `storagedevicemodule\`, `uicontroller\`
- `drw_main\eupemaker\pe_maker\` — WinPE builder

**Key controllers / engine-side classes (embedded in the UI process):**
- Scan: `CScanMgr : IScanMgr`, `CDoScanPartition`, `CDoScanLostPartition`, `EnumCollectPartition`, `CQuickScanCompleteWnd`
- Devices: `CStorageDeviceWithCache`, `CWin32Disk`, `CWin32Part`, `CVolumeDevice`, `CSimple/CMirror/CSpanned/CStriped/CPartitionVirtual/CRaid5VolumeDevice`, `CVHDDisk`, `CVHDXDisk`, `CImageFileDevice(+WithDecompress)`, `CSimulateStorageDevice`, `CRemoteDevice`, `CCacheDeviceProxy`
- FS analyzers (inside UI!): `CNtfsVolumeAnalyze`, `CReFSVolumeAnalyze`, `CBtrfsVolumeAnalyzer`, `CHfsPlusVolumeAnalyer`, `CZfsVolumeAnalyze`, `CPartitionTableAnalyze`, `ExFatCalcPartition`, `CCheckNtfs/Fat/ExFat/BitlockerPartition` (recoverability analysis), data-run readers (resident/non-resident NTFS, ReFS, RAW)
- RAID: `CRaid0/1/5/6/10/Jbod(+Impl)`, `CUserRaid`
- Recovery: `CRecoveryMgr`, `CRecoveryFilter`, `CRecoveryIOBase`, `CFileRecoverWorker`+`RecoveryParams`, `CSaveFailedFilesWithHtml`, `EndRecoverThread`
- Results models: `CTree/CTreeControl/CTreeDataModel/CTreeNode/CTreeViewModel(+)`, `CClassifyTreeViewController/Model` (Type view), `CListViewModel`, `CRightContent`, `VirtualSearchTree`+`TreeFolderNode`, `LostTreeModel/LostTableModel`, `RecommendFileNodeView`
- Filter/sort: `CAdvanceFileFilterMgr`, `CFilterLostFilesBtn`, `CSizeFilterBtn`, `CTimeFilterBtn`, `CTypeFilterBtn`, `CSetFilterThread`, `CSortNodeThread`, `CSortPopupMenu`, `CUserDefineSortBar`, `CSetClassifyThread`, `CMatchingFileTypeWorker`
- Thumbnails/preview: `CTreeNode→INodeThumbnail`, `CGetListThumbnailThread`, `CCameraThumbnail`, preview stubs `Image/AV/Pdf/Office/Archive/Mem PreviewStub`, `CPreviewController`, `PreviewMainDialog`, `ArchivePreviewModel/View/Window`
- Cloud: `CCloudDevice`, `CCloudLogginThread`, `CCloudRecoveryController`, `CloudSearchController`, `CCloudDataRunList`, `item_sector_info@CCloudDevice`
- NAS/Linux: `CNasDevicePage/EnummingPage/LogonPage/...`, `CLinux*`, `CNasWidgetsMgr`, `CNasConnectThread`, `CNasDeviceMgr`
- Modules: `CSDCardWidgetsMgr`, `CPEWidgetsMgr`+`CPEUILogic`+`CPEUISink`+`CPeMaker`, `CRaidWidgetsMgr`, `CBitlockerWidgetsMgr`+`BitlockerUnlockThread`+`CCheckBitlockerPartition`, `CSBackupMgr` (Todo/disk backup cross-sell), `CAutoRecoverMgr`
- AI: `DrwAIDedupWindow`, `AIAssistantTipDialog`, `AIDeduplicationTipMenu`, `LocalAiSecureIconLabel` ("Local AI | Secure")
- Upsell: `CBuyGuideWnd`, `InnerBuyActivateDlg`, `CRecoverLimitDlg` (free quota 2 GB gate: "%1 free recovery remaining")

## 4. Widget-layer names (visible control taxonomy)

Titlebar: TitleClose/ShowMin/ShowNormal/ShowMaxed/Activate/Vip/Version/SurVey/Shared/Service/OnlineHelp/OnlineChat/NotifyMessage/LoadScan/SaveScan/NeedHelp Buttons + Menu.
Results: `CSearchBar`+`CSearchLineedit`, `CSelAllCheckBox(Ex)`, `CAdvancedFilterBtn`, `NavigateBar`+`NavigationTabBar`, `CUserDefineSortBar`, `AnalysisGridWidget` (thumbnail grid), `CRightIconView/CRightListView`, `LostLeftTreeView/LostRightTableView`, `CClassifyLeftTreeView`, `RecommendViewJumpPathBtn`, `TitleLoadScanButton`.
Scan view: `CScanCommonPopDlg`, `CScanMgr`, `PcRecoveryTimer`, `QRoundProgressBar`+`RadiusProgressBar`, `ScanStopContentChildFrame`, `FreezeAnimalDialog` (the frozen-fox animation during filtering!), `CScanCommonPopDlg`.
Home: `DrwHomePage`, `DrwHomePartListView` (partition cards), `NoDeviceGuide*` (empty states), `LostDisksSupportButton`, `DiskRefreshButton`.
Recovery: `ChooseDestinationPathDialog`+`ChooseDestinationPathItem` (dest list w/ Local/Cloud tabs, "SSD (C:)" rows, space check, source-drive warning), `RecoverProgressDialog`, `UnllocateProgressLabel`, `CRecoveryArea`.
Common: `CSVGPushButton`, `CRoundRadiusLabel`, `DoubleLineTextButton`, `IconTextButton`, `IconCustomButton`, `ArrowToggleButton`, `GifWaitingButton/Label`, `DrwFlyoutWindow/Menu`, `PublicShadowDialog`, `InnerShadowWidget`, `FollowShadowDialog`, `CToolTipController`, `CShadowMenu`, `CPopupMenu`, `FlowLayout`, `EmbedIeWnd` (embedded IE for web panels), `QWinTaskbarButton` (taskbar progress).

## 5. Theme engine mechanics

- Two RCC bundles `:/Resource/...` (light) and `:/ResourceDark/...` (dark) swapped at runtime; `CToolTipController`/`CustomTranslator`/`CThemeMenu(Title/Item)` per-theme switch, `onChangeCurQSS`, `CustomTranslator` reloads .qm on language switch, `CLanguageMgr`.
- QSS (Qt stylesheets) built from `themdark.ini` token table (118 tokens) — indexed `1..118` strings include full gradients + QSS fragments (`border:1px solid #0560fd;`).
- High-DPI on; `FontStyleFixer` fixes fonts per-locale; window 1240×840 default (min 800×600), remembers state.
- Per-locale fonts: Segoe UI/Segoe UI Semibold 600 (en), Meiryo UI (ja), Malgun Gothic (ko), Microsoft YaHei (zh-CN).

## 6. Left-tree node taxonomy (from ImageResource.xml + icons)

Root level: Disk · Partition existing/lost × lock/unlock · System · USB existing/lost × lock/unlock · RAID/RAID0 · desktop · SD · UAV · tachograph · camera · nas · recyclebin.
Group nodes: **Deleted Files · Lost Files · Directory Intact · File Name Lost · File Path Lost · Tags Extracted · Unimportant · Unsaved · bookmark · Dropbox · Google Drive · OneDrive**.
Type nodes (with 24px + disabled states): img · video · doc · audio · Archive · others · Email.
Drive-type icon map: type 0-49 covers system/USB/partition × existing/lost × lock/unlock + camera_sony/UAV/SD/desktop/recyclebin/folder/USB/HD/tachograph (+ BitLocker variants).

## 7. File filter taxonomy (FileFilter.xml)

8 filter categories: Graphic (icon 0) · Document (1) · Email (2, pst/dbx) · Audio (3) · Video (4) · Other (5) · Archive (6) · Bookmarks (7). Each with extension lists (TIFF group, camera RAW groups: nef/arw/cr2/sr2/raw/rw2/dng/raf/crw/dcr/kdc/erf/3fr/mef/pef/srw/orf/mrw/mdc/ptx/x3f/mos/cr3/heic…).

## 8. Free-version business rules (to parity-port as "free tier" gates)

- "%1 free recovery capacity" / "Free version recovers up to %1 data" (2 GB)
- "Only %1 free recovery remaining." / "No free recovery remaining." / "You exceeded your free quota. Get Pro Now!"
- Trial = preview-only: "Trial version is preview-only."
- "15s preview only" for large media, "Files larger than 100MB are not available for preview."
- Activation: license code XXXXX-XXXXX-XXXXX-XXXXX-XXXXX, auto/manual activation, "praise" (rating) dialogs.

## 9. Misc mechanics worth porting

- Duplicate files notice: "we tried different scanning methods. Duplicate files may be found" → AI Dedup feature
- Browser-cache detection (browser filetype icons: Chrome/Edge/Firefox/Safari/360/liebao)
- "Hide 0KB Files / Hide Temp Files / Hide System Files / Hide Browser Cache" toggles
- Scan-result email export: `CSaveFailedFilesWithHtml` (recovery report HTML)
- `CSysCheckMac/CSysChkMotherBoard` hardware fingerprint for licensing
- Crash guards: DRW_Memory_Crash_{guid}, DRW_Crash_{guid}, DRW0_Mutex_610044_ChengDu single-instance
- Language: 22 locales, `CustomTranslator`, RTL Arabic support
