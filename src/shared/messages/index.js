// 消息总线：只做订阅与转发。入队、去重、广播都在 Rust，这里不保存队列，
// 因此一次发布只会出现一条。

import { messageDismiss, messagePublish, onMessage, onMessageDismissed } from "../api/index.js";

const sinks = new Set();
const dismissHandlers = new Set();
let connected = null;

/** 注册显示 sink；sink 抛错只记日志，不影响其它订阅者。 */
export function registerSink(sink) {
  sinks.add(sink);
  return () => sinks.delete(sink);
}

/** 语义更直白的别名，供页面订阅消息。 */
export const subscribe = registerSink;

/** 订阅撤销事件；横幅用它提前退场。 */
export function subscribeDismissed(handler) {
  dismissHandlers.add(handler);
  return () => dismissHandlers.delete(handler);
}

/** 发布一条消息，返回后端分配后的消息体。 */
export async function publish(message) {
  return messagePublish(message);
}

/** 撤销一条消息。 */
export async function dismiss(id) {
  return messageDismiss(id);
}

/** 订阅后端广播；重复调用不会重复订阅。 */
export async function connect() {
  if (connected) return connected;
  const stops = await Promise.all([
    onMessage((message) => dispatch(message)),
    onMessageDismissed((payload) => {
      for (const handler of dismissHandlers) {
        try {
          handler(payload);
        } catch (error) {
          console.error("[lumina-tiles] 消息撤销订阅失败", error);
        }
      }
    })
  ]);
  connected = () => {
    for (const stop of stops) stop();
    connected = null;
  };
  return connected;
}

function dispatch(message) {
  for (const sink of sinks) {
    try {
      sink(message);
    } catch (error) {
      console.error("[lumina-tiles] 消息订阅失败", error);
    }
  }
}
