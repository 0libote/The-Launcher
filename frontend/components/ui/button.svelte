<script lang="ts" module>
  import { cva, type VariantProps } from 'class-variance-authority';

  /** Button styles. Variants map to the shared classes in style.css. */
  export const buttonVariants = cva('', {
    variants: {
      variant: {
        primary: 'primary',
        secondary: 'secondary',
        ghost: 'text-button',
        danger: 'text-button danger',
        icon: 'icon-button',
      },
    },
    defaultVariants: { variant: 'primary' },
  });
  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import { cn } from '../../lib/utils';

  let {
    variant = 'primary',
    class: klass = '',
    children,
    ...rest
  }: {
    variant?: ButtonVariant;
    class?: string;
    children?: Snippet;
  } & Omit<HTMLButtonAttributes, 'class' | 'children'> = $props();
</script>

<button class={cn(buttonVariants({ variant }), klass)} {...rest}
  >{@render children?.()}</button
>
