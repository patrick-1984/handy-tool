const WINDOWS_APPLICATION_CONTROL_BLOCKED =
  "flm_windows_application_control_blocked";

export const isFlmBlockedByWindowsApplicationControl = (
  error: string | null | undefined,
): boolean => error?.includes(WINDOWS_APPLICATION_CONTROL_BLOCKED) ?? false;
