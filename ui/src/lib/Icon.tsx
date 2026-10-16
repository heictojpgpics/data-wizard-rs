/**
 * Icon set — hand-rolled inline SVGs (lucide-style geometry: 24x24
 * viewBox, 2px round stroke, currentColor). Zero external assets or
 * dependencies; the set stays small and purpose-built for this app.
 *
 * Usage: <Icon size={16} /> — every icon accepts size + optional className.
 */
import type { SVGProps } from "react";

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "width" | "height"> {
  size?: number;
}

function make(path: React.ReactNode, displayName: string) {
  const C = ({ size = 16, ...rest }: IconProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      focusable="false"
      {...rest}
    >
      {path}
    </svg>
  );
  C.displayName = displayName;
  return C;
}

/* ── Storage / devices ────────────────────────────────────────────────── */

export const HardDriveIcon = make(
  <>
    <line x1="22" x2="2" y1="12" y2="12" />
    <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
    <line x1="6" x2="6.01" y1="16" y2="16" />
    <line x1="10" x2="10.01" y1="16" y2="16" />
  </>,
  "HardDrive",
);

export const UsbIcon = make(
  <>
    <circle cx="10" cy="7.5" r="1.5" />
    <circle cx="10" cy="12" r="1.5" />
    <circle cx="10" cy="16.5" r="1.5" />
    <path d="M10 7.5V16.5" />
    <path d="M12.5 7.5h4l2 3.5-2 3.5h-4" />
    <path d="M14 10.5l-1.5.75 1.5.75" />
  </>,
  "Usb",
);

export const DiscIcon = make(
  <>
    <circle cx="12" cy="12" r="10" />
    <circle cx="12" cy="12" r="2" />
  </>,
  "Disc",
);

export const DatabaseIcon = make(
  <>
    <ellipse cx="12" cy="5" rx="9" ry="3" />
    <path d="M3 5v14a9 3 0 0 0 18 0V5" />
    <path d="M3 12a9 3 0 0 0 18 0" />
  </>,
  "Database",
);

export const FolderIcon = make(
  <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />,
  "Folder",
);

export const FolderOpenIcon = make(
  <>
    <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />
  </>,
  "FolderOpen",
);

export const FileIcon = make(
  <>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
  </>,
  "File",
);

export const ImageIcon = make(
  <>
    <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
    <circle cx="9" cy="9" r="2" />
    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
  </>,
  "Image",
);

export const VideoIcon = make(
  <>
    <path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" />
    <rect x="2" y="6" width="14" height="12" rx="2" />
  </>,
  "Video",
);

export const AudioIcon = make(
  <>
    <path d="M2 10v3" />
    <path d="M6 6v11" />
    <path d="M10 3v18" />
    <path d="M14 8v7" />
    <path d="M18 5v13" />
    <path d="M22 10v3" />
  </>,
  "Audio",
);

export const FileTextIcon = make(
  <>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
    <path d="M16 13H8" />
    <path d="M16 17H8" />
    <path d="M10 9H8" />
  </>,
  "FileText",
);

export const ArchiveIcon = make(
  <>
    <rect width="20" height="5" x="2" y="3" rx="1" />
    <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
    <path d="M10 12h4" />
  </>,
  "Archive",
);

export const MailIcon = make(
  <>
    <rect width="20" height="16" x="2" y="4" rx="2" />
    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
  </>,
  "Mail",
);

export const CodeIcon = make(
  <>
    <path d="m16 18 6-6-6-6" />
    <path d="m8 6-6 6 6 6" />
  </>,
  "Code",
);

export const FileQuestionIcon = make(
  <>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
    <circle cx="10" cy="14" r="1.2" fill="currentColor" stroke="none" />
  </>,
  "FileQuestion",
);

/* ── Wizard / flow ────────────────────────────────────────────────────── */

export const WandIcon = make(
  <>
    <path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72" />
    <path d="m14 7 3 3" />
    <path d="M5 6v4" />
    <path d="M19 14v4" />
    <path d="M10 2v2" />
    <path d="M7 8H3" />
    <path d="M21 16h-4" />
    <path d="M11 3H9" />
  </>,
  "Wand",
);

export const SearchIcon = make(
  <>
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </>,
  "Search",
);

export const ShieldCheckIcon = make(
  <>
    <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />
    <path d="m9 12 2 2 4-4" />
  </>,
  "ShieldCheck",
);

export const ShieldIcon = make(
  <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />,
  "Shield",
);

export const ScanSearchIcon = make(
  <>
    <path d="M3 7V5a2 2 0 0 1 2-2h2" />
    <path d="M17 3h2a2 2 0 0 1 2 2v2" />
    <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
    <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
    <circle cx="11" cy="11" r="3.2" />
    <path d="m16.5 16.5 3 3" />
  </>,
  "ScanSearch",
);

export const DownloadIcon = make(
  <>
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" x2="12" y1="15" y2="3" />
  </>,
  "Download",
);

export const RotateCcwIcon = make(
  <>
    <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
    <path d="M3 3v5h5" />
  </>,
  "RotateCcw",
);

export const HistoryIcon = make(
  <>
    <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
    <path d="M3 3v5h5" />
    <path d="M12 7v5l4 2" />
  </>,
  "History",
);

export const ClockIcon = make(
  <>
    <circle cx="12" cy="12" r="10" />
    <polyline points="12 6 12 12 16 14" />
  </>,
  "Clock",
);

/* ── Actions / UI chrome ──────────────────────────────────────────────── */

export const XIcon = make(
  <path d="M18 6 6 18M6 6l12 12" />,
  "X",
);

export const CheckIcon = make(
  <path d="M20 6 9 17l-5-5" />,
  "Check",
);

export const ChevronRightIcon = make(
  <path d="m9 18 6-6-6-6" />,
  "ChevronRight",
);

export const ChevronDownIcon = make(
  <path d="m6 9 6 6 6-6" />,
  "ChevronDown",
);

export const ChevronLeftIcon = make(
  <path d="m15 18-6-6 6-6" />,
  "ChevronLeft",
);

export const ArrowLeftIcon = make(
  <>
    <path d="m12 19-7-7 7-7" />
    <path d="M19 12H5" />
  </>,
  "ArrowLeft",
);

export const ArrowRightIcon = make(
  <>
    <path d="M5 12h14" />
    <path d="m12 5 7 7-7 7" />
  </>,
  "ArrowRight",
);

export const RefreshIcon = make(
  <>
    <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
    <path d="M3 3v5h5" />
    <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
    <path d="M16 16h5v5" />
  </>,
  "Refresh",
);

export const SunIcon = make(
  <>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2" />
    <path d="M12 20v2" />
    <path d="m4.93 4.93 1.41 1.41" />
    <path d="m17.66 17.66 1.41 1.41" />
    <path d="M2 12h2" />
    <path d="M20 12h2" />
    <path d="m6.34 17.66-1.41 1.41" />
    <path d="m19.07 4.93-1.41 1.41" />
  </>,
  "Sun",
);

export const MoonIcon = make(
  <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />,
  "Moon",
);

export const PaletteIcon = make(
  <>
    <circle cx="13.5" cy="6.5" r=".8" fill="currentColor" stroke="none" />
    <circle cx="17.5" cy="10.5" r=".8" fill="currentColor" stroke="none" />
    <circle cx="8.5" cy="7.5" r=".8" fill="currentColor" stroke="none" />
    <circle cx="6.5" cy="12.5" r=".8" fill="currentColor" stroke="none" />
    <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z" />
  </>,
  "Palette",
);

export const TrashIcon = make(
  <>
    <path d="M3 6h18" />
    <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
    <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
  </>,
  "Trash",
);

export const AlertTriangleIcon = make(
  <>
    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
  </>,
  "AlertTriangle",
);

export const InfoIcon = make(
  <>
    <circle cx="12" cy="12" r="10" />
    <path d="M12 16v-4" />
    <path d="M12 8h.01" />
  </>,
  "Info",
);

export const CircleCheckIcon = make(
  <>
    <circle cx="12" cy="12" r="10" />
    <path d="m9 12 2 2 4-4" />
  </>,
  "CircleCheck",
);

export const CircleXIcon = make(
  <>
    <circle cx="12" cy="12" r="10" />
    <path d="m15 9-6 6" />
    <path d="m9 9 6 6" />
  </>,
  "CircleX",
);

export const SaveIcon = make(
  <>
    <path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
    <path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" />
    <path d="M7 3v4a1 1 0 0 0 1 1h7" />
  </>,
  "Save",
);

export const LayersIcon = make(
  <>
    <path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z" />
    <path d="m6.08 11.4-3.5 1.6a1 1 0 0 0 0 1.81l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9a1 1 0 0 0 0-1.83l-3.5-1.59" />
  </>,
  "Layers",
);

export const ExternalLinkIcon = make(
  <>
    <path d="M15 3h6v6" />
    <path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  </>,
  "ExternalLink",
);

export const PlayIcon = make(
  <polygon points="6 3 20 12 6 21 6 3" />,
  "Play",
);

export const SparklesIcon = make(
  <>
    <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
  </>,
  "Sparkles",
);

export const LockIcon = make(
  <>
    <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
  </>,
  "Lock",
);

export const SettingsIcon = make(
  <>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </>,
  "Settings",
);

export const SlidersIcon = make(
  <>
    <line x1="4" x2="4" y1="21" y2="14" />
    <line x1="4" x2="4" y1="10" y2="3" />
    <line x1="12" x2="12" y1="21" y2="12" />
    <line x1="12" x2="12" y1="8" y2="3" />
    <line x1="20" x2="20" y1="21" y2="16" />
    <line x1="20" x2="20" y1="12" y2="3" />
    <line x1="2" x2="6" y1="14" y2="14" />
    <line x1="10" x2="14" y1="8" y2="8" />
    <line x1="18" x2="22" y1="16" y2="16" />
  </>,
  "Sliders",
);

/* ── Window caption glyphs (Windows 11 exact geometry) ────────────────── */

export const CaptionMinimizeIcon = ({ size = 15, ...rest }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} aria-hidden {...rest}>
    <path d="M5 12h14" />
  </svg>
);
CaptionMinimizeIcon.displayName = "CaptionMinimize";

export const CaptionMaximizeIcon = ({ size = 15, ...rest }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} aria-hidden {...rest}>
    <rect x="5.5" y="5.5" width="13" height="13" rx="1.5" />
  </svg>
);
CaptionMaximizeIcon.displayName = "CaptionMaximize";

export const CaptionRestoreIcon = ({ size = 15, ...rest }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} aria-hidden {...rest}>
    <path d="M8.5 8.5V6.2c0-.94.76-1.7 1.7-1.7h7.6c.94 0 1.7.76 1.7 1.7v7.6c0 .94-.76 1.7-1.7 1.7h-2.3" />
    <rect x="4.5" y="8.5" width="11" height="11" rx="1.5" />
  </svg>
);
CaptionRestoreIcon.displayName = "CaptionRestore";

export const CaptionCloseIcon = ({ size = 15, ...rest }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} aria-hidden {...rest}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);
CaptionCloseIcon.displayName = "CaptionClose";

/* ── Brand mark ───────────────────────────────────────────────────────── */

/** The Data Wizard brand glyph: a disc with a rising recovery arc.
 * Painted with currentColor so it follows text contexts; the ink
 * variant uses the gradient via CSS (.dw-brand-mark). */
export function BrandGlyph({ size = 22, ...rest }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden {...rest}>
      <circle cx="12" cy="12" r="9.2" stroke="currentColor" strokeWidth="1.9" opacity="0.92" />
      <path
        d="M12 6.6a5.4 5.4 0 1 0 5.4 5.4"
        stroke="currentColor"
        strokeWidth="2.1"
        strokeLinecap="round"
      />
      <path d="M17.4 12 14.6 9.2M17.4 12l2.8-2.8" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round" opacity="0.55" />
      <circle cx="12" cy="12" r="1.6" fill="currentColor" />
    </svg>
  );
}

/** Category icon by extension bucket. */
export function categoryIcon(cat: string) {
  switch (cat) {
    case "photos":
      return ImageIcon;
    case "videos":
      return VideoIcon;
    case "audio":
      return AudioIcon;
    case "documents":
      return FileTextIcon;
    case "archives":
      return ArchiveIcon;
    case "email":
      return MailIcon;
    case "code":
      return CodeIcon;
    default:
      return FileIcon;
  }
}

/**
 * Category glyph — the render-safe way to draw a category icon: a
 * module-level component with static icon references per branch (the
 * react compiler forbids aliasing components through runtime calls
 * during render, e.g. `const Icon = categoryIcon(cat)`).
 */
export function CategoryGlyph({
  cat,
  size = 16,
  ...rest
}: { cat: string } & IconProps) {
  switch (cat) {
    case "photos":
      return <ImageIcon size={size} {...rest} />;
    case "videos":
      return <VideoIcon size={size} {...rest} />;
    case "audio":
      return <AudioIcon size={size} {...rest} />;
    case "documents":
      return <FileTextIcon size={size} {...rest} />;
    case "archives":
      return <ArchiveIcon size={size} {...rest} />;
    case "email":
      return <MailIcon size={size} {...rest} />;
    case "code":
      return <CodeIcon size={size} {...rest} />;
    default:
      return <FileIcon size={size} {...rest} />;
  }
}
