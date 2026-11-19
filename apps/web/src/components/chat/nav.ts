// Where the conversation list and the chats can send the person.

export interface ChatNav {
  openBot: (id: string) => void
  openExchange: (a: string, b: string) => void
  openBatch: (id: string) => void
  askMany: () => void
  /** Opens a bot at one message of one of its conversations. */
  openMessage: (bot: string, thread: string, ms: number) => void
  add: () => void
  openChannel: (id: string) => void
}
