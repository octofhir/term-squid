import { type Component, JSX, splitProps } from 'solid-js'
import styles from './Button.module.css'

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'sm' | 'md' | 'lg'
}

export const Button: Component<ButtonProps> = (props) => {
  const [local, others] = splitProps(props, ['variant', 'size', 'class', 'children'])
  const variant = local.variant ?? 'primary'
  const size = local.size ?? 'md'

  return (
    <button
      class={`${styles.button} ${styles[variant]} ${styles[size]} ${local.class || ''}`}
      {...others}
    >
      {local.children}
    </button>
  )
}