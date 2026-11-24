import type { ReactNode } from 'react';

export const VERSION = '0.1.3';

export const REPO = 'https://github.com/greenbugx/ANSImuz';

export const LINKS = {
  repo: REPO,
  releases: `${REPO}/releases/latest`,
  issues: `${REPO}/issues`,
  readme: `${REPO}#readme`,
  contributing: `${REPO}/blob/main/CONTRIBUTING.md`,
  license: `${REPO}/blob/main/LICENSE`,
  thirdParty: `${REPO}/blob/main/THIRD-PARTY-LICENSES.md`,
  conduct: `${REPO}/blob/main/CODE_OF_CONDUCT.md`,
  copr: 'https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/',
  sourceforge: 'https://sourceforge.net/projects/ansimuz/files/latest/download',
  mpv: 'https://mpv.io/',
  ytdlp: 'https://github.com/yt-dlp/yt-dlp',
};

export const SECTIONS = [
  { id: 'features', index: '01', label: 'Features' },
  { id: 'docs', index: '02', label: 'Docs' },
  { id: 'download', index: '03', label: 'Download' },
  { id: 'faq', index: '04', label: 'FAQ' },
] as const;

export const SECTION_IDS = SECTIONS.map((section) => section.id);

export type Feature = {
  title: string;
  description: ReactNode;
};

export const FEATURES: Feature[] = [
  {
    title: 'Multi-source streaming',
    description: 'Search YouTube Music and Spotify, then play both from a single unified queue.',
  },
  {
    title: 'Native libmpv engine',
    description:
      'In-process audio via native libmpv with headless stream prefetching and zero external player windows.',
  },
  {
    title: 'Synchronized lyrics',
    description:
      'Words and syllables highlight in time with the track. Duets, background vocals, and manual scroll are built in.',
  },
  {
    title: 'Terminal album art',
    description:
      'Covers render in adaptive Unicode pixel blocks or HQ terminal graphics, including monochrome artwork.',
  },
  {
    title: 'Queue & history',
    description:
      'Add, remove, and reorder upcoming tracks, or jump back through anything played in your session.',
  },
  {
    title: 'Offline cache',
    description:
      'Audio streams, album covers, and lyrics save locally for instant replay even when offline.',
  },
  {
    title: 'Color themes',
    description:
      'Sample dynamic palettes live from album covers, or keep the classic retro green ANSI look.',
  },
  {
    title: 'Discord Rich Presence',
    description:
      'Broadcast live track progress, buttons, and auto-hosted cover art to Discord (experimental).',
  },
  {
    title: 'Command palette',
    description: (
      <>
        Press <kbd>:</kbd> to search and trigger every command, mode, and setting from one place.
      </>
    ),
  },
];

export type Control = {
  key: string;
  action: string;
};

export const CONTROLS: Control[] = [
  { key: 'space', action: 'Play or pause the current track' },
  { key: '← / →', action: 'Previous track or next in queue' },
  { key: '+ / -', action: 'Adjust playback volume (±5%)' },
  { key: 'Tab', action: 'Switch focus between search and queue' },
  { key: 'A / Del', action: 'Add search result or remove queued track' },
  { key: 'L', action: 'Open or close the lyrics view' },
  { key: ':', action: 'Open the command palette' },
  { key: 'S', action: 'Open settings' },
  { key: 'X', action: 'Toggle queue shuffle' },
  { key: 'R', action: 'Cycle repeat mode' },
  { key: '?', action: 'Show the about overlay' },
  { key: 'Q', action: 'Quit ANSImuz' },
];

export type Platform = {
  name: string;
  meta: string;
  commandLabel?: string;
  commands?: string[];
  note?: ReactNode;
  action: string;
  href: string;
  primary?: boolean;
  secondary?: {
    label: string;
    href: string;
  };
};

export const PLATFORMS: Platform[] = [
  {
    name: 'Fedora',
    meta: 'COPR repository',
    commandLabel: 'install',
    commands: ['sudo dnf copr enable greenbugx/ansimuz', 'sudo dnf install ansimuz'],
    note: (
      <>
        Includes native libmpv and yt-dlp automatically. Or download the prebuilt binary
        from{' '}
        <a href={LINKS.releases} target="_blank" rel="noreferrer">
          GitHub Releases
        </a>
        .
      </>
    ),
    action: 'Open COPR',
    href: LINKS.copr,
  },
  {
    name: 'Arch Linux',
    meta: 'PKGBUILD',
    commandLabel: 'build',
    commands: [
      'paru -S python-spotifyscraper',
      'git clone https://github.com/greenbugx/ANSImuz.git',
      'cd ANSImuz/packaging/arch && makepkg -si',
    ],
    note: (
      <>
        Installs libmpv (via mpv) and yt-dlp automatically. Use yay or manual AUR if you
        do not use paru, or grab a release from{' '}
        <a href={LINKS.releases} target="_blank" rel="noreferrer">
          GitHub Releases
        </a>
        .
      </>
    ),
    action: 'View packaging',
    href: `${REPO}/tree/main/packaging/arch`,
  },
  {
    name: 'Windows',
    meta: 'Portable x64 build',
    commandLabel: 'prerequisite',
    commands: ['winget install --id yt-dlp.yt-dlp -e'],
    note: (
      <>
        Bundles the Python runtime and native <kbd>libmpv-2.dll</kbd> with no separate mpv
        install needed. Extract the archive and run <kbd>ansimuz.exe</kbd> on Windows 10
        or 11.
      </>
    ),
    action: 'Download .rar',
    href: LINKS.releases,
    primary: true,
    secondary: {
      label: 'SourceForge mirror',
      href: LINKS.sourceforge,
    },
  },
  {
    name: 'From source',
    meta: 'Rust + C bridge',
    note: (
      <>
        Run <kbd>scripts/install_linux.sh</kbd> or <kbd>scripts/install_windows.ps1</kbd>,
        or see <kbd>CONTRIBUTING.md</kbd> for manual Rust, Python, and libmpv setup.
      </>
    ),
    action: 'Build guide',
    href: LINKS.contributing,
  },
];

export type Faq = {
  question: string;
  answer: ReactNode;
};

export const FAQS: Faq[] = [
  {
    question: 'Do I need a Spotify or YouTube account?',
    answer:
      'No. ANSImuz searches public catalog data and plays the result. You do not need to log in to either service.',
  },
  {
    question: 'Do I need Python installed?',
    answer:
      'Not for the Windows portable build. It bundles an embedded Python runtime and its dependencies. The Linux packages wire up system Python modules for you. Building from source uses the virtual environment workflow in CONTRIBUTING.md.',
  },
  {
    question: 'Do I need mpv and yt-dlp installed?',
    answer: (
      <>
        Only <kbd>yt-dlp</kbd> is required on your PATH to resolve audio streams. Playback
        runs in-process through native <kbd>libmpv</kbd> instead of launching the{' '}
        <kbd>mpv</kbd> CLI: Windows portable builds bundle <kbd>libmpv-2.dll</kbd>{' '}
        directly, Fedora and Arch packages install it for you, and Debian/Ubuntu users
        just need <kbd>libmpv2</kbd>.
      </>
    ),
  },
  {
    question: 'Is Discord Rich Presence stable?',
    answer:
      'It is experimental. It displays live track info, progress, stream buttons, and auto-hosted cover art, though minor rough edges remain. You can toggle it anytime from Settings.',
  },
  {
    question: 'Where does it store caches?',
    answer: (
      <>
        Under <kbd>$XDG_CACHE_HOME/ansimuz</kbd> on Linux or{' '}
        <kbd>%LOCALAPPDATA%\ansimuz</kbd> on Windows. Audio streams, artwork thumbnails,
        lyrics, and Discord artwork caches live there. You can clear them anytime from
        Settings.
      </>
    ),
  },
  {
    question: 'Does it run on Windows?',
    answer:
      'Yes. Download the self-contained portable release for Windows 10 and 11, or run through WSL2 if you prefer a Linux environment.',
  },
  {
    question: 'Is it open source?',
    answer:
      'Yes. ANSImuz is released under the MIT license. The source, packaging scripts, issues, and release notes are all on GitHub.',
  },
  {
    question: 'Will album art work in my terminal?',
    answer:
      'Yes. Any terminal with ANSI color support works via adaptive Unicode pixel blocks, and terminals with native graphics protocols can switch Art Mode to HQ in Settings for sharp covers.',
  },
];

export const ASCII_LOGO = `   ▄████████ ███▄▄▄▄      ▄████████  ▄█    ▄▄▄▄███▄▄▄▄   ███    █▄   ▄███████▄  
  ███    ███ ███▀▀▀██▄   ███    ███ ███  ▄██▀▀▀███▀▀▀██▄ ███    ███ ██▀     ▄██ 
  ███    ███ ███   ███   ███    █▀  ███▌ ███   ███   ███ ███    ███       ▄███▀ 
  ███    ███ ███   ███   ███        ███▌ ███   ███   ███ ███    ███  ▀█▀▄███▀▄▄ 
▀███████████ ███   ███ ▀███████████ ███▌ ███   ███   ███ ███    ███   ▄███▀   ▀ 
  ███    ███ ███   ███          ███ ███  ███   ███   ███ ███    ███ ▄███▀       
  ███    ███ ███   ███    ▄█    ███ ███  ███   ███   ███ ███    ███ ███▄     ▄█ 
  ███    █▀   ▀█   █▀   ▄████████▀  █▀    ▀█   ███   █▀  ████████▀   ▀████████▀ `;
