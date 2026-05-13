import packageJson from "../package.json";

export type ThemeMode = "system" | "light" | "dark";

export type AppSettings = {
  themeMode: ThemeMode;
  defaultNdiGroups: string;
  hideDonationPrompt: boolean;
};

export const DEFAULT_APP_SETTINGS: AppSettings = {
  themeMode: "system",
  defaultNdiGroups: "",
  hideDonationPrompt: false,
};

export const APP_VERSION = packageJson.version;

export const REPO_URL = "https://github.com/MikanseiLaboratory/momaku";
export const DONATION_URL = "https://subs.twitch.tv/flowingspdg";
export const LP_URL = "https://mikanseilaboratory.github.io/";
