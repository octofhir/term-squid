import { type Component, JSX, splitProps } from 'solid-js'
import styles from './Input.module.css'

interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label?: string
  error?: string
}

export const Input: Component<InputProps> = (props) => {
  const [local, others] = splitProps(props, ['label', 'error', 'class'])

  return (
    <div class={styles.container}>
      {local.label && <label class={styles.label}>{local.label}</label>}
      <input
        class={`${styles.input} ${local.error ? styles.error : ''} ${local.class || ''}`}
        {...others}
      />
      {local.error && <span class={styles.errorText}>{local.error}</span>}
    </div>
  )
}