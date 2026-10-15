# Glossary

Terms used throughout this documentation.

| Term | Meaning |
|---|---|
| **BPB** | BIOS Parameter Block. The filesystem-specific metadata in the boot sector (FAT, exFAT, NTFS). |
| **B-tree** | Balanced tree. Used by HFS+ catalog, ReFS object table, Btrfs root tree for indexing. |
| **Carving** | Recovering files by scanning raw disk bytes for file-type magic numbers, without relying on filesystem metadata. |
| **Cluster** | The allocation unit of a filesystem. A cluster is N sectors (N = power of 2). |
| **CNID** | Catalog Node ID. HFS+'s inode number equivalent. |
| **DASD** | Direct Access Storage Device. `FSCTL_ALLOW_EXTENDED_DASD_IO` lets us read past the live filesystem's logical end. |
| **Deep scan** | Quick scan + raw carving on unallocated regions. Slow but finds more. |
| **EOCD** | End of Central Directory. The ZIP archive trailer record. |
| **EOI** | End of Image. The JPEG trailer marker (`FF D9`). |
| **extents** | A list of `(logical_offset, physical_offset, length)` triples describing where a file's data lives. Used by HFS+, Btrfs, ext4 (in extent-tree mode). |
| **FAT** | File Allocation Table. The cluster-chain index in FAT12/16/32. Also the family name. |
| **FAT chain** | A linked list of cluster numbers, terminated by 0xFFFFFFFF, that describes a file's data location. |
| **GPT** | GUID Partition Table. The modern partition format. |
| **IEND** | Image End. The PNG trailer chunk. |
| **inode** | Index node. ext2/3/4's per-file metadata structure. |
| **JFIF** | JPEG File Interchange Format. The most common JPEG sub-format. |
| **LFN** | Long File Name. The VFAT extension that stores long names alongside the 8.3 short name. |
| **LBA** | Logical Block Address. The sector number on a disk. |
| **MFT** | Master File Table. NTFS's per-file metadata structure. One 1024-byte record per file. |
| **MFT record** | A 1024-byte entry in the MFT describing one file. |
| **MOS** | Meta-Object Set. ZFS's per-dataset object tree root. |
| **NVD** | Next VDev. ZFS's device label structure. |
| **Quick scan** | Metadata-only walk of the filesystem. Fast but only finds files the filesystem still knows about. |
| **ReFS** | Resilient File System. Microsoft's modern server filesystem (Server 2012+). |
| **Resident attribute** | An NTFS attribute whose data fits inside the MFT record (small files). |
| **Run list** | NTFS's encoding of non-resident attribute data runs. Compressed `(length, offset)` pairs. |
| **SMART** | Self-Monitoring, Analysis and Reporting Technology. Disk health metrics. |
| **SOI** | Start of Image. The JPEG header marker (`FF D8`). |
| **UBA** | Uberblock. ZFS's superblock equivalent, transaction-numbered. |
| **USN journal** | Update Sequence Number journal. NTFS's per-volume log of file system changes. |
| **VBR** | Volume Boot Record. The first sector of a volume. Contains the BPB. |
| **Vdev** | Virtual Device. ZFS's device abstraction (mirror, raidz, disk, file). |
| **WinPE** | Windows Preinstallation Environment. The minimal Windows used by rescue bootable media. |
| **xattr** | Extended attribute. Per-file metadata key/value pairs (NTFS alternate data streams, ext4 xattrs, macOS resource forks). |
