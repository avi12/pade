// Open a dashboard link - a summary's, a pull request's - in the user's browser,
// never in the dashboard window itself.

import { Command, sendMessage } from "@/lib/ipc/messaging";

export async function openLink(url: string) {
  try {
    await sendMessage(Command.openUrl, { url });
  } catch (error) {
    console.warn("pade: could not open the dashboard link", error);
  }
}
