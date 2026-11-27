import { useEffect, useState } from 'react'

const preference = 'ftw.mail.sync-minutes'
const eventName = 'ftw-sync-frequency'
const choices = [0, 5, 10, 30, 60]

function readFrequency() {
  const stored = localStorage.getItem(preference)
  const value = stored === null ? 10 : Number(stored)
  return choices.includes(value) ? value : 10
}

export function useSyncFrequency(): [number, (value: number) => void] {
  const [frequency, setFrequency] = useState(readFrequency)
  useEffect(() => {
    const update = () => setFrequency(readFrequency())
    window.addEventListener(eventName, update)
    return () => window.removeEventListener(eventName, update)
  }, [])
  function save(value: number) {
    localStorage.setItem(preference, String(value))
    window.dispatchEvent(new Event(eventName))
  }
  return [frequency, save]
}
