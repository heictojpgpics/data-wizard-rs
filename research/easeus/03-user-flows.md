# EaseUS DRW — Complete User-Flow Reconstruction (UI/UX Spec Source)

Reconstructed from 1,556 QM UI strings + widget names + metaobject signals + configs + icons.
Every screen, control, state, and transition the user experiences, in order.

---

## FLOW 0 — Launch

1. Splash/loading: "Loading, Please wait a moment..." / "Loading device information" / "Loading disk information..." (device enumeration, ~1-3 s, auto-refresh disk `Auto refresh disk`).
2. Single-instance guard; crash-recovery dialogs if prior session crashed.
3. **Home (source select)** — `DrwHomePage`:
   - Top: TitleBar (logo, version, buttons: Menu · Activate/VIP · Load Scan · Save Scan · Online Help · Online Chat · Notify · Survey · Share · Need Help · Settings(menu)).
   - **Module rail (left)**: `DrwHomePartListView` nav with 8 modules (icons Nav_ico_{PC,SD,NAS,RAID,Cloud,Bitlocker,Crashed,PE} nor/sel/dis):
     1. PC recovery (default) — "Data Recovery"
     2. SD Card — "Recover SD Card" (photos/videos from cameras, UAV/dash-cams)
     3. NAS & Linux — remote SSH connect
     4. RAID — reconstruct RAID 0/1/5/6/10/JBOD from member disks
     5. Cloud Drive — Dropbox/Google Drive/OneDrive/iCloud
     6. BitLocker — encrypted partition unlock
     7. Crashed PC (WinPE) — create bootable media
     8. (+ Disk Backup cross-sell)
   - **Main area (PC module)**: "Select a location where your data was lost to scan and locate the deleted or missing files." +
     **drive card list** (Disk/Partition rows): icon by type (system/USB/SD/camera/UAV/tachograph/desktop/recyclebin/folder/HD × existing/lost × lock states),
     name ("SSD (C:)" / "Disk 1"), capacity bar, "Internal drives (%1)" / "External drives (%1)" / "Unallocated" group headers,
     **"Search for lost files" CTA per row** + row hover states.
     - Special rows: "Can't find location?" / "Not sure which drive to scan → Scan entire disk" / "Specify the folder you want to recover data from" (folder picker) / "Import a disk image" (*.vmdk *.vhd *.vhdx *.edisk *.dmg *.img) / Recycle Bin quick row ("Can't find the data in the Recycle Bin?").
     - Grouping: "Devices"/"Locations"/"Guess you want" (recent: "Your last search rescords goes here", "Continue last scanning" resume).
   - Footer: "More Recovery Tools" + "Advanced Data Recovery" (upsells), feedback.
   - Empty/broken states: "Unable to find your hard drive?" → troubleshooting flyout (Q1/Q2/refresh, "Refresh drives", "Remove all external devices", "Auto refresh disk paused for security").

## FLOW 1 — Scan (after clicking Search/Scan)

**ScanningView** (per source):
- **Stage A: Quick Scan** — "Quick Scanning%1, Files Found:" (progress % + live file counter).
  Circular + linear progress; elapsed "Elapsed time:" / "Remaining time: %1"; "Scanning disk sectors: %1".
  Tip carousel: "Quick Scanning will take a few minutes to find deleted files and lost partitions." · "It is highly probable to find the data after the scan is completed." · "You can preview and recover your files during the scanning process."
- **Live events**: lost partitions discovered → `sigFoundQuickPartitonNotify` → "Woohoo! Lost partitions found..." → left tree refresh + toast "Lost bitlocker partition found (%1)".
- **Controls**: Pause/Resume ("Scan Paused%1, Files found:"), Stop ("Stop searching for lost data?" with data-loss warning), Minimize, "Change Scan Location" (re-scan dialog), disk-image suggestion on failing disks (bad sectors → BadSectorTipsDlg "Program is trying to skip bad sectors to find data.").
- **Stage B: Quick Scan Complete** → `CQuickScanCompleteWnd` banner card: "Congratulations! Scan Completed!" — actually quick-complete is an *interstitial*: "Quick Scan Completed" + content: left (what quick found + "Advanced Scan can find more files - many users recover their data after a full scan." + "Advanced Scan is trying to find more files for you." + estimated time "Estimated Time for Advanced Scan:") right (deep-scan illustration + "Auto-Recover after Advanced Scan Completed" toggle + "Keep Scanning" CTA). User can skip to browse results NOW.
- **Stage C: Advanced/Deep Scan** — "Advanced Scanning%1, Files Found:" sector-by-sector ("Advanced Scan is searching files sector by sector to ensure data recovery as much as possible.") + RAW carving ("Smart scan in progress, analyzing file chunks information." = carving by signature, "Scanned disk sectors: %1").
  Auto-transitions to results view; scanning indicator stays in header/titlebar.
- **Post-scan organization**: "Organizing Found Files..." / "Organizing found files..." (CSortNodeThread/CSetClassifyThread) → then filter-thread states ("Filtering", "It may take a while to filter desired files for you", freeze animation `FreezeAnimalDialog`, "FilterFreezeFinished").
- Session autosave: "Save Scan Session" (rsf) + resume prompts.

## FLOW 2 — Results browsing

**Layout**: 3-pane. Left = location tree; Top = navigate bar; Center = file list/grid; Right = (optional preview).
- **Left tree** (`CLeftTreeView`/`CTreeViewController`, `VirtualSearchTree`):
  - Root: source disk/partition nodes (existing+lost, lock states, RAID/desktop/SD/UAV/tachograph/camera/nas/recyclebin)
  - **Special groups**: Deleted Files · Lost Files · Existing Files · **Other Lost Files** (Other deleted files) · Directory Intact (folders with structure preserved) · **File Name Lost** (RAW files without names, "Here are some files whose original name cannot be found, but are more likely to be your desired data.") · **File Path Lost** ("Files we found with Advanced Scan, but the file path was fully or partially missing...") · **Tags Extracted** ("We classify and organize the found files by extracted tags... camera, artist, creator...") · Unimportant Files ("To help you find the desired files quickly, we organize some unimportant files here.") · Unsaved File · Duplicates · Recycle Bin · Cloud drives (Dropbox/Google Drive/OneDrive) · bookmark/Email/Archive type folders.
  - Type groups inside: Pictures/Videos/Audio/Docuents/Archive/Email/Others (+24px icons, dis states).
- **Navigate bar** (`CNavigateBar`): breadcrumb (path icons per node type) + back/forward history (`NavigateHistroyCtrl`) + **search box** ("Search files or folders", live: `onSearchTextChanged` → "Searched "%1 files (%2)" for "%3"" / "Searched nothing..." + empty-state advice: "1.Try less specific keywords 2.Ensure words are spelled correctly...") + **Filter & Search button** ("Filter your desired files by file type, size and date modified, etc.") + view toggles (list/grid) + Group by (Type/Date Modified/Tag) + Sort menu (Name/Size/Date Modified, Ascending/Descending) + "Switch views anytime to see all files."
- **Filter panel** (`CAdvanceFileFilterMgr` drawer): File Type tree (8 categories from FileFilter.xml, multi-select with checkboxes + counts), File Size (All/<512KB/1MB~512KB/512MB~1GB/>1GB), Date Modified (All/Today/Yesterday/Last 7 days/Last 1 month/custom range), **Advanced Filter** (custom extension entry "Enter the file type suffix, such as *.doc; *.xlsx; *.ppt" + size spinners + date pickers), "Lost Files Only" toggle, "Reset Filter", "Hide 0KB Files / Hide Temp Files / Hide System Files / Hide Browser Cache" toggles. Live re-filter with progress ("Filtering").
- **File list** (`CListViewModel`/`AnalysisGridWidget`):
  - Columns: checkbox · name (+type icon 24px) · size · date modified · (+recoverability state, tag caption, thumbnails on grid view).
  - Row hover: preview button + checkbox; double-click = preview; right-click context menu (`CShowContextMenu`/`CShadowMenu`): Open · Preview · Recover · Recover All · Select All/Clear · Open file location · Copy path · properties.
  - Live updates during ongoing scan: rows stream in (`OnSinkInsertNode`), counters update, "Some files might be incomplete while scanning. We highly recommend recovering files after the scan is completed."
  - Selection bar: "Selected: %1 files %2" / "Select All (%1/%2)" · "Recover Now"/"Recover All..." CTA (gradient) + "Recover Remaining".
  - AI features: "AI Recommend" badge + "AI Recommended" group ("AI Recommend is on. Showing %1 recommended files." — highlights most-worth-recovering), "AI Deduplication" (dedupe toggle: "Filters duplicates to show only the single best file per area."), "AI Recovery Insight:" summary.
- **Preview** (`PreviewMainDialog`): full-screen modal; type-specific renderers: Image (zoom/pan, EXIF), Video (player + 15s trial limit), Audio (waveform + player), PDF/Office/Archive (embedded viewers), text (from TxtViewFiles list), camera RAW. Loading sequences per type (PreviewLoadingView + PngSequence/LoadingImg|Audio|Doc|Video). Failures: "Oops, file is not available for preview." · "File format does not support preview." · "Files larger than 100MB are not available for preview." · corrupted → "Click here to repair the file if needed." (repair upsell) · "But this file is likely recoverable based on its structure."
- **Partition check flow** (lost partition rows): "Partition Recoverable"/"Intelligently identify whether the partition can be directly recovered" → `PartitionCheckDialog` (analysis progress) → `PartitionCheckRetDialog`: "Good News! Partition Recoverable" → "Restore Partition" (whole-partition rescue, "Instantly restore recoverable lost partitions without waiting for the scan to finish.") or "The partition cannot be rescued directly, try to recover the data directly" → data-only recovery. Result: "%1 partition and its data were successfully recovered!" / failure → "We couldn't restore the partition, but no data was modified."

## FLOW 3 — Recovery

1. **Select files → Recover** (CTA or context).
2. **ChooseDestinationPathDialog**: tabs **Local / Cloud**; drive list rows ("SSD (C:)" + free space + capacity bar), folder browse ("Browse For Folder", "Choose folder..."), free-space validation ("Need additional %1 of space. Please free up space or select another drive." / "Insufficient disk space"), **source-drive protection**: "Do not save data to this drive" / "Source Drive! Change another one." / "Data might be %1 and %2 if you save files to original drive (where you lost data)." + "We highly recommend saving the found data to a different drive or a network drive." · "Cannot save to original partition". Cloud tab: "You can select a Cloud drive to save the found data." (with quota checks). "Change" to re-pick. CTA "Recover".
3. **RecoverProgressDialog**: "Recovering: Disk %1" — per-file stream "Recovering: name" + counters "Recovered: %1/ %2 files" · "To be recovered: %1 files" · speed; failure list panel "Below are files failed to be recovered:" + reasons ("Possible reasons for failed recovery: 1.Insufficient storage space. 2.The recovery path is too long. 3.The file name contains garbage characters. 4.The sector reading location exceeds total sector number..."); cancel-confirm ("Sure to Stop Recovering?" — "Remaining data can't be recovered if you stop."); BitLocker decrypting state ("Automatically Decrypting").
4. **Completion**: success page "Recovery Successful!" + summary card: "Recovered: %1 files (%2)" · destination "Recovered to:" + "Open file location"/"Open in Explorer" CTA · "View Recovered" · "Do more" (share/rate) · recovery report (HTML, `CSaveFailedFilesWithHtml`).
5. **Free-tier gate** (free version): 2 GB cap — during selection "Free version recovers up to %1 data." · "You have %1 free recovery remaining." · at cap: "You exceeded your free quota. Get Pro Now!" → upsell (`CRecoverLimitDlg`, gradient CTA "Recover Unlimited Data", "1 users choose to activate Pro and successfully recover data." social proof).
6. Post-recovery guidance: "Recover Sooner for Better Chances" · "Task Handover: From Recovery to Protection" (backup cross-sell) · "Don't let it happen again. Build your safety net now."

## FLOW 4 — auxiliary modules (parity scope: PC + SD + session; optional: NAS/RAID/BitLocker)

- **SD Card**: connect guidance ("Connect your SD card to PC/Laptop with a card reader..."), device feedback ("SD card connected but not detected?" troubleshooting), camera direct ("Recover data directly from camera. SDcard and USB drive etc.").
- **NAS/Linux**: IP/user/port SSH form, "NAS searching from local network", enumming animation, volumes ("Nas scanning defaults to scanning the maximum volume and can switch to scanning different volumes").
- **RAID**: "Select a RAID to search for lost data", manual/automatic reconstruction, "Rapid volume scan to completely get rid of troublesome RAID reconstruction".
- **BitLocker**: "Select a location to unlock Bitlocker and start recovering" · password/48-bit key entry · "Try Automatically Unlock" (general keys + Microsoft account keys + local keys: "Automatically unlock Bitlocker drives with general keys, local keys and keys in the Microsoft account.") · "Search Without Unlock" · auto-decrypt animation.
- **Cloud**: "Select a Cloud Drive to Start Recovery" — OAuth browser flow ("Please authorize login on the webpage"), drives: OneDrive/Dropbox/Google Drive/iCloud ("Sign in to iCloud" with Apple ID + 2FA code).
- **WinPE (Crashed PC)**: "Select a USB drive" → "Creating bootable drive..." → BIOS instructions.

## Global UX details (port-grade specifics)

- **Confirm dialogs** (DrwMessageBox pattern): title + icon + body + primary/secondary CTA; every destructive action has a warning variant ("Sure to Exit Program?" — "Valuable data might not be recovered if you exit.").
- **Tooltips** everywhere (CToolTipController, delayed).
- **Titlebar** double-line text (product + dynamic subtitle), custom caption buttons, VIP state pill.
- **Taskbar progress** integration (QWinTaskbarButton + taskbar progress).
- **Reduced-distraction details**: "Don't show this again" option on tips; "Got it" coach marks (IntroductionWidget — first-run tour); UserGuidTipWnd.
- **Theme switch animation** (QSS cross-fade), language switch without restart.
- **Numbers formatting**: %1 Bytes/KB/MB/GB/TB with adaptive units; time "00:00:00" clock; "%p%" progress; dates localized.
- **Empty states**: "This folder is empty!" · "No LOST or RAW partitions were found that could be recovered." · search/filter no-results with advice lists.
- **Error toasts + notification center** (CNotifyCenter + TitleNotifyMessageButton).
- **Accessibility-ish**: big hit areas, hover states, focus outlines, drag-restore window, keyboard nav in lists.

## The "Satisfying UX" checklist (EaseUS's psychology, to replicate)

1. Progress is ALWAYS visible & honest (%, counts, sectors, ETA).
2. Results stream live — never a blank wait.
3. Reassurance copy at anxiety points (data loss warnings, "highly recoverable").
4. CTA hierarchy: gradient primary, ghost secondary; one primary action per screen.
5. Empty/error states teach the next action.
6. Free-tier gates are soft (preview everything, gate on save).
7. Post-success moment is celebratory + educational (share, rate, protect).
