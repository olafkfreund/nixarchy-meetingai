import { MANAGED_PLAN } from "./site";

export interface FaqEntry {
  question: string;
  answer: string;
}

export const HOME_FAQS: readonly FaqEntry[] = [
  {
    question: "Does Lennette join my call as a bot?",
    answer:
      "No. Lennette runs on your computer and captures microphone and system audio without adding a bot to your meeting. Live transcription and AI answers are processed by your configured providers.",
  },
  {
    question: "Can others see Lennette when I share my screen?",
    answer:
      "Windows capture exclusion works with capture methods that honor display affinity. On Linux, it requires our patched Hyprland setup. Ordinary Linux sessions do not hide the overlay. Test your meeting app and share mode before relying on it.",
  },
  {
    question: "Which meeting apps can I use with Lennette?",
    answer:
      "Lennette captures audio from your computer rather than connecting to a specific meeting service. Configure microphone and system audio for your device, then test it with your meeting app.",
  },
  {
    question: "How do I ask for help during a meeting?",
    answer:
      "Show or hide the overlay with Ctrl + Shift + Backslash. Type a question, or start listening to get suggestions for recognized questions using the meeting transcript and your selected context.",
  },
  {
    question: "Where are my conversations and transcripts stored?",
    answer:
      "Chat and meeting history is stored locally on your device. You can revisit it in the dashboard. Relevant context is sent to your AI provider when you request an answer.",
  },
];

export const PRICING_FAQS: readonly FaqEntry[] = [
  {
    question: "What do I pay for on the free plan?",
    answer:
      "Nothing to Lennette. Add your own AI provider key for answers and a Deepgram key for live transcription. Those providers bill you directly for API usage. You do not need a Lennette account.",
  },
  {
    question: "What will Managed include?",
    answer: `The planned ${MANAGED_PLAN.price} monthly subscription includes 2,000 AI responses and 30 hours of live transcription. Each follow-up answer counts toward the same response allowance, shared across your devices. You will not need separate AI or transcription API keys.`,
  },
  {
    question: "Does my audio stay on my device?",
    answer:
      "Live transcription sends your audio to Deepgram. AI requests send relevant conversation context and any attached content to your selected provider. Your chat and meeting history is stored locally on your device.",
  },
  {
    question: "Can I sign up for Managed now?",
    answer:
      "Managed access is coming soon. Join the waitlist to hear when it opens. You can download Lennette and use your own keys today.",
  },
];
