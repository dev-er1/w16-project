//! w16-langs\w16-cc\src\value\f80.rs
//!
//! # Тип Long Double из C
//!
//! Long Double занимает 80 бит.

/// 80-битное дробное число
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct F80 {
    /// Знаковый бит (false = положительное, true = отрицательное число)
    pub sign: bool,

    /// Экспонента (15 бит, храним в u16). Маскируется как 0x7FFF.
    /// Смещение экспоненты (bias) для 80-бит составляет 16383.
    pub exponent: u16,

    /// Мантисса (64 бита). В формате x87 старший бит (бит 63) —
    /// это явная целая часть (обычно 1 для нормализованных чисел).
    pub significand: u64,
}

impl F80 {
    /// Константа смещения экспоненты по стандарту для 80 бит
    pub const BIAS: u16 = 16383;

    /// Создать ноль
    pub fn zero(sign: bool) -> Self {
        Self {
            sign,
            exponent: 0,
            significand: 0,
        }
    }

    /// Проверка: является ли число нулем
    pub fn is_zero(&self) -> bool {
        self.exponent == 0 && self.significand == 0
    }

    /// Приводит число к нормализованному виду (старший бит мантиссы должен быть 1).
    /// Также обрабатывает недокументированные состояния, нулевые мантиссы и потерю порядка.
    pub fn normalize(mut self) -> Self {
        // Если мантисса равна 0, то это либо честный 0, либо ушедшее в поднормализованную зону число.
        if self.significand == 0 {
            self.exponent = 0;
            return self;
        }

        // Сколько ведущих нулей в нашей 64-битной мантиссе?
        let leading_zeros = self.significand.leading_zeros() as u16;

        if leading_zeros == 0 {
            // Старший бит уже 1, число нормализовано.
            return self;
        }

        if self.exponent > leading_zeros {
            // Экспоненты хватает, чтобы компенсировать сдвиг
            self.exponent -= leading_zeros;
            self.significand <<= leading_zeros;
        } else {
            // Экспоненты не хватает -> число становится поднормализованным (Subnormal)
            // Сдвигаем мантиссу на остаток экспоненты, сама экспонента зануляется
            if self.exponent > 0 {
                self.significand <<= self.exponent - 1;
            }
            self.exponent = 0;
        }

        self
    }

    /// Конвертирует стандартное f64 (double) в F80 (long double).
    /// Отлично подходит для написания тестов и парсинга простых литералов.
    pub fn from_f64(num: f64) -> Self {
        if num == 0.0 {
            return Self::zero(num.is_sign_negative());
        }

        let bits = num.to_bits();

        // 1. Извлекаем знак
        let sign = (bits >> 63) != 0;

        // 2. Извлекаем экспоненту как знаковое число для безопасной математики
        let f64_exponent = ((bits >> 52) & 0x7FF) as i32;

        // 3. Извлекаем мантиссу
        let f64_significand = bits & 0xF_FFFF_FFFF_FFFF;

        let (exponent, significand) = if f64_exponent == 0 {
            // Субнормализованное число в f64
            let mut f80_exp = (1 - 1023 + 16383) as u16;
            let mut f80_sig = f64_significand << 11;

            let shift = f80_sig.leading_zeros() as u16;
            f80_sig <<= shift;
            f80_exp -= shift;

            (f80_exp, f80_sig)
        } else {
            // Нормализованное число
            let implicit_one = 1 << 52;
            let full_sig = f64_significand | implicit_one;

            // Выравниваем мантиссу по левому (63-му) биту u64
            let f80_sig = full_sig << 11;

            // Безопасно считаем смещение через i32
            let f80_exp = (f64_exponent - 1023 + 16383) as u16;

            (f80_exp, f80_sig)
        };

        Self {
            sign,
            exponent,
            significand,
        }
    }
}

/// Простое сложение для f80
/// Точное сложение для f80 с использованием 128-битной точности для сдвигов
impl std::ops::Add for F80 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        if self.is_zero() {
            return rhs;
        }
        if rhs.is_zero() {
            return self;
        }

        // Строго определяем, какое число БОЛЬШЕ ПО МОДУЛЮ до сдвигов
        let is_self_greater = if self.exponent != rhs.exponent {
            self.exponent > rhs.exponent
        } else {
            self.significand >= rhs.significand
        };

        let (exp_max, exp_min, sig_max, sig_min, sign_max, sign_min) = if is_self_greater {
            (
                self.exponent,
                rhs.exponent,
                self.significand,
                rhs.significand,
                self.sign,
                rhs.sign,
            )
        } else {
            (
                rhs.exponent,
                self.exponent,
                rhs.significand,
                self.significand,
                rhs.sign,
                self.sign,
            )
        };

        let exp_diff = exp_max - exp_min;

        if exp_diff >= 64 {
            return if self.exponent >= rhs.exponent {
                self
            } else {
                rhs
            };
        }

        // Расширяем мантиссы до 128 бит и смещаем влево на 32 бита,
        // чтобы не терять улетающие при сдвиге вправо младшие разряды
        let sig_max_128 = (sig_max as u128) << 32;
        let mut sig_min_128 = (sig_min as u128) << 32;

        sig_min_128 >>= exp_diff;

        let res_sign = sign_max;
        let mut res_exponent = exp_max;
        let mut res_significand_128: u128;

        if sign_max == sign_min {
            res_significand_128 = sig_max_128 + sig_min_128;
            if res_significand_128 >= (1 << (64 + 32)) {
                res_significand_128 >>= 1;
                res_exponent += 1;
            }
        } else {
            res_significand_128 = sig_max_128 - sig_min_128;
        }

        // Математическое округление к ближайшему целому (добавляем половину веса отсекаемой части)
        let round_addon = 1 << 31;
        res_significand_128 += round_addon;
        let mut res_significand = (res_significand_128 >> 32) as u64;

        // Если округление вызвало цепной перенос разряда, обнулив мантиссу
        if res_significand == 0 && res_significand_128 > round_addon {
            res_significand = 1 << 63;
            res_exponent += 1;
        }

        F80 {
            sign: res_sign,
            exponent: res_exponent,
            significand: res_significand,
        }
        .normalize()
    }
}

/// Вычитание для F80 (a - b = a + (-b))
impl std::ops::Sub for F80 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        // Вычитание просто меняет знак правого операнда и складывает
        let negated = F80 {
            sign: !rhs.sign,
            exponent: rhs.exponent,
            significand: rhs.significand,
        };
        self + negated
    }
}

/// Унарный минус для F80
impl std::ops::Neg for F80 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        F80 {
            sign: !self.sign,
            exponent: self.exponent,
            significand: self.significand,
        }
    }
}

/// Умножение для F80
impl std::ops::Mul for F80 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        // Оба нулевые?
        if self.is_zero() || rhs.is_zero() {
            return F80::zero(self.sign != rhs.sign);
        }

        // Знак результата: XOR исходных знаков
        let result_sign = self.sign != rhs.sign;

        // Экспоненты складываются (но с учётом bias)
        // new_exp = exp1 + exp2 - BIAS
        let mut result_exponent =
            (self.exponent as i32) + (rhs.exponent as i32) - (F80::BIAS as i32);

        // Мантиссы перемножаются в 128-битной арифметике
        let prod_128 = (self.significand as u128) * (rhs.significand as u128);

        // В F80 significand хранится в масштабе 2^63. Произведение двух
        // таких significand-ов имеет масштаб 2^126, поэтому базовый сдвиг
        // вправо равен 63. После этого нормализуем возможный диапазон [1, 4).
        let mut result_significand_128 = prod_128 >> 63;
        let remainder = prod_128 & ((1u128 << 63) - 1);

        // Round to nearest по отброшенным 63 битам.
        if remainder >= (1u128 << 62) {
            result_significand_128 += 1;
        }

        while result_significand_128 >= (1u128 << 64) {
            result_significand_128 >>= 1;
            result_exponent += 1;
        }

        if result_exponent <= 0 {
            return F80::zero(result_sign);
        }

        if result_exponent >= 0x7FFF {
            return F80 {
                sign: result_sign,
                exponent: 0x7FFF,
                significand: 1u64 << 63,
            };
        }

        F80 {
            sign: result_sign,
            exponent: result_exponent as u16,
            significand: result_significand_128 as u64,
        }
        .normalize()
    }
}

/// Деление для F80
impl std::ops::Div for F80 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        // Деление на ноль?
        if rhs.is_zero() {
            // Возвращаем бесконечность (максимальная экспонента)
            return F80 {
                sign: self.sign != rhs.sign,
                exponent: 0x7FFF,
                significand: 1u64 << 63,
            };
        }

        // Делимое является нулём?
        if self.is_zero() {
            return F80::zero(self.sign != rhs.sign);
        }

        // Знак результата
        let result_sign = self.sign != rhs.sign;

        // Экспоненты: new_exp = exp1 - exp2 + BIAS
        let mut result_exponent =
            (self.exponent as i32) - (rhs.exponent as i32) + (F80::BIAS as i32);

        // В x87 significand хранится как 1.xxx в масштабе 2^63.
        // Поэтому делим мантиссы так, чтобы quotient сразу был кандидатом
        // на 64-битную F80-мантиссу:
        //
        //     quotient = (sig_a / sig_b) * 2^63
        //
        // Так как sig_a/sig_b лежит примерно в диапазоне [0.5, 2),
        // quotient может оказаться ниже явного integer-bit или, в редком
        // случае, на один бит выше. Ниже нормализуем это с поправкой экспоненты.
        let dividend_128 = (self.significand as u128) << 63;
        let divisor_128 = rhs.significand as u128;

        let mut quotient = dividend_128 / divisor_128;
        let remainder = dividend_128 % divisor_128;

        // Round to nearest: если отброшенная часть >= половины делителя,
        // поднимаем младший бит результата.
        if remainder.saturating_mul(2) >= divisor_128 {
            quotient += 1;
        }

        if quotient == 0 {
            return F80::zero(result_sign);
        }

        while quotient < (1u128 << 63) {
            quotient <<= 1;
            result_exponent -= 1;
        }

        while quotient >= (1u128 << 64) {
            quotient >>= 1;
            result_exponent += 1;
        }

        if result_exponent <= 0 {
            return F80::zero(result_sign);
        }

        if result_exponent >= 0x7FFF {
            return F80 {
                sign: result_sign,
                exponent: 0x7FFF,
                significand: 1u64 << 63,
            };
        }

        F80 {
            sign: result_sign,
            exponent: result_exponent as u16,
            significand: quotient as u64,
        }
        .normalize()
    }
}
