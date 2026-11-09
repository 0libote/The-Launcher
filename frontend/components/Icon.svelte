<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    Check,
    ChevronDown,
    Clock,
    Download,
    Eye,
    Folder,
    Gamepad2,
    Heart,
    Image,
    Info,
    Layers,
    LayoutGrid,
    Library,
    List,
    Monitor,
    Moon,
    PenLine,
    Play,
    Plus,
    RotateCw,
    Search,
    Settings,
    Sun,
    X,
  } from 'lucide-svelte';

  let { name, size = 20 }: { name: string; size?: number } = $props();

  type LucideIcon = typeof Library;
  const icons: Record<string, LucideIcon> = {
    library: Library,
    grid: LayoutGrid,
    list: List,
    shelf: Layers,
    heart: Heart,
    clock: Clock,
    search: Search,
    plus: Plus,
    down: ChevronDown,
    arrow: ArrowRight,
    play: Play,
    close: X,
    settings: Settings,
    folder: Folder,
    download: Download,
    monitor: Monitor,
    gamepad: Gamepad2,
    eye: Eye,
    edit: PenLine,
    image: Image,
    check: Check,
    refresh: RotateCw,
    info: Info,
    back: ArrowLeft,
    sun: Sun,
    moon: Moon,
  };
  const IconComponent = $derived(icons[name] ?? Gamepad2);
</script>

<IconComponent {size} />
