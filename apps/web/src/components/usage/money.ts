import { locale } from '../../i18n'

/** Dollars in full, as the person's locale writes them (US$ 1.234,56 or $1,234.56). */
export const money = (n: number, digits = 2): string =>
  n.toLocaleString(locale, { style: 'currency', currency: 'USD', minimumFractionDigits: digits, maximumFractionDigits: digits })
