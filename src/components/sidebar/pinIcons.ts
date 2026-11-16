import {
  Bell,
  Bookmark,
  Brain,
  Clock,
  FileText,
  Flag,
  Folder,
  Globe,
  Heart,
  Keyboard,
  Link2,
  Mic,
  Pin,
  Rocket,
  Settings2,
  Sparkles,
  Star,
  Tag,
  Wrench,
  Zap,
} from "lucide-react";

type IconComponent = React.ComponentType<{
  width?: number | string;
  height?: number | string;
  className?: string;
}>;

/** The icons a pinned page can have (Advanced › Appearance), by stored key. */
export const PIN_ICONS: Record<string, IconComponent> = {
  pin: Pin,
  star: Star,
  bookmark: Bookmark,
  heart: Heart,
  zap: Zap,
  flag: Flag,
  bell: Bell,
  folder: Folder,
  file: FileText,
  mic: Mic,
  keyboard: Keyboard,
  settings: Settings2,
  sparkles: Sparkles,
  brain: Brain,
  globe: Globe,
  clock: Clock,
  tag: Tag,
  rocket: Rocket,
  wrench: Wrench,
  link: Link2,
};

export const DEFAULT_PIN_ICON = "pin";

/** A stored key the app no longer knows shows the default icon. */
export const pinIcon = (key: string | undefined): IconComponent =>
  PIN_ICONS[key ?? ""] ?? PIN_ICONS[DEFAULT_PIN_ICON];
