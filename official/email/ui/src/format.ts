export function senderName(from: string) {
  const match = from.match(/^\s*"?([^"<]*?)"?\s*</);
  const name = match?.[1]?.trim();
  return name || from.split("@")[0] || from || "未知发件人";
}

export function senderAddress(from: string) {
  return from.match(/<([^>]+)>/)?.[1] || from;
}

export function initial(from: string, email: string) {
  return (senderName(from) || email).trim().charAt(0).toUpperCase() || "?";
}

export function shortDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  return sameDay
    ? date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}

export function fullDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
