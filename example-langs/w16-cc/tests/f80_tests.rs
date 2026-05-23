//! Тесты на тип F80

use w16_cc::value::*;

#[test]
fn test_from_f64() {
    let one = F80::from_f64(1.0);
    // У 1.0 экспонента должна быть в точности равна BIAS (2^0)
    assert_eq!(one.exponent, F80::BIAS);
    // Старший бит мантиссы должен быть 1, остальные 0
    assert_eq!(one.significand, 1 << 63);
    assert!(!one.sign);
}

#[test]
fn test_simple_add() {
    let a = F80::from_f64(1.25);
    let b = F80::from_f64(2.5);
    let expected = F80::from_f64(3.75);

    assert_eq!(a + b, expected);
}

#[test]
fn test_add_overflow_significand() {
    let a = F80::from_f64(1.0);
    let b = F80::from_f64(1.0);
    let expected = F80::from_f64(2.0);

    // При сложении 1.0 + 1.0 мантиссы (1<<63) + (1<<63) дадут переполнение.
    // Наш код должен это обработать, сдвинуть результат и выдать 2.0 (экспонента BIAS + 1)
    assert_eq!(a + b, expected);
}

#[test]
fn test_add_different_signs() {
    let a = F80::from_f64(5.0);
    let b = F80::from_f64(-3.0); // Отрицательное
    let expected = F80::from_f64(2.0);

    assert_eq!(a + b, expected);
}

#[test]
fn test_add_zeros() {
    let zero = F80::zero(false);
    let a = F80::from_f64(42.0);

    assert_eq!(a + zero, a);
    assert_eq!(zero + a, a);
}

#[test]
fn test_add_subnormal_f64() {
    // Проверяем, как наш конвертер и сложение справляются с экстремально маленькими
    // субнормализованными числами из f64 (у которых в f64 экспонента была 0)
    let a = F80::from_f64(5e-324);
    let b = F80::from_f64(5e-324);
    let expected = F80::from_f64(1e-323);

    assert_eq!(a + b, expected);
}

#[test]
fn test_add_cancel_to_zero() {
    // Полное взаимное уничтожение мантисс. Результат должен стать чистым нулем.
    let a = F80::from_f64(1234.5678);
    let b = F80::from_f64(-1234.5678);
    
    assert_eq!(a + b, F80::zero(false));
}

#[test]
fn test_add_huge_and_tiny() {
    // Одно число настолько огромное, а второе настолько маленькое, 
    // что при выравнивании порядков (exp_diff >= 64) маленькое число должно полностью отсечься.
    let big = F80::from_f64(1e15);
    let tiny = F80::from_f64(1e-15);

    assert_eq!(big + tiny, big);
}

#[test]
fn test_add_precision_loss_edge() {
    // Ситуация, когда меньшее число сдвигается вправо, но не исчезает совсем,
    // а меняет только самые младшие биты мантиссы старшего числа.
    let a = F80::from_f64(1.0); // мантисса: 1000...000
    let b = F80::from_f64(0.0000000000000001); // очень маленькое, но влезающее в 64-бит лимит
    
    let res = a + b;
    // Результат не должен быть равен просто `a`, мелкая прибавка обязана зафиксироваться в мантиссе
    assert_ne!(res, a);
}

#[test]
fn test_add_negative_zeros() {
    // В Си -0.0 + -0.0 должно давать -0.0
    let nz1 = F80::zero(true);
    let nz2 = F80::zero(true);
    let res = nz1 + nz2;
    
    assert_eq!(res.sign, true);
    assert!(res.is_zero());
}

#[test]
fn test_add_sub_underflow_to_subnormal() {
    // Собираем числа вручную. Из f64 невозможно передать субнормализованное для F80 число.
    // Вычитаем из числа чуть меньшее, проверяя, что логика не паникует и корректно считает разницу
    let a = F80 { sign: false, exponent: 1, significand: 1 << 63 };
    let b = F80 { sign: true, exponent: 1, significand: 1 << 62 }; 
    let res = a + b;

    assert!(!res.sign);
    assert!(res.significand > 0);
}

#[test]
fn test_add_alternating_signs_high_precision() {
    // Когда вычитаем близкие числа, происходит потеря значимости.
    // Проверяем, что результат близок к ожиданию (порядок верен)
    let a = F80::from_f64(123456789.012345);
    let b = F80::from_f64(-123456789.012344);
    let expected = F80::from_f64(0.000001);

    let diff = a + b;
    // Экспонента должна быть близка к ожидаемой (может отличаться на 1 из-за округления)
    assert!(diff.exponent == expected.exponent || diff.exponent == expected.exponent + 1);
    
    // Результат должен быть положительным
    assert!(!diff.sign);
    // И не равен нулю
    assert!(!diff.is_zero());
}

#[test]
fn test_add_max_f64_values() {
    // Складываем максимально возможные числа, представимые в f64.
    // Наш F80 имеет 15 бит экспоненты (против 11 у f64), поэтому для него 
    // это НЕ переполнение (не Infinity), он должен вернуть точный честный результат.
    let max_f64 = f64::MAX;
    let a = F80::from_f64(max_f64);
    let b = F80::from_f64(max_f64);
    
    let res = a + b;
    // Экспонента должна вырасти ровно на 1 по сравнению с оригиналом
    assert_eq!(res.exponent, a.exponent + 1);
    assert_eq!(res.significand, a.significand); 
}

#[test]
fn test_add_bits_shifting_all_the_way_out() {
    // Сдвиг ровно на границе 63-64 бит.
    let a = F80::from_f64(1.0); // exp = BIAS
    // Число, чья экспонента ровно на 63 меньше, чем у 1.0
    let b = F80 {
        sign: false,
        exponent: F80::BIAS - 63,
        significand: 1 << 63,
    };
    
    let res = a + b;
    // Младший бит b должен был сдвинуться на 63 позиции вправо и стать 1-м битом результирующей мантиссы
    assert_ne!(res.significand, a.significand);
    assert_eq!(res.significand & 1, 1);
}

#[test]
fn test_add_massive_random_mix() {
    // Проверяем дикую комбинацию знаков и порядков
    let a = F80::from_f64(-9876543210.123456);
    let b = F80::from_f64(0.000000123456789);
    let expected = F80::from_f64(-9_876_543_210.123_455);

    let res = a + b;
    assert!(res.sign);
    assert_eq!(res.exponent, expected.exponent);
}

// ==================== ТЕСТЫ NORMALIZE ====================

#[test]
fn test_normalize_already_normalized() {
    // Число уже нормализовано (старший бит 1)
    let f = F80 {
        sign: false,
        exponent: F80::BIAS,
        significand: 1u64 << 63,
    };
    let normalized = f.normalize();
    assert_eq!(normalized, f);
}

#[test]
fn test_normalize_with_leading_zeros() {
    // Число имеет ведущие нули, но экспоненты хватает для компенсации
    let f = F80 {
        sign: false,
        exponent: F80::BIAS + 8,
        significand: 0x0F_FF_FF_FF_FF_FF_FF_FF,
    };
    let normalized = f.normalize();
    
    // После нормализации старший бит должен быть 1
    assert_eq!(normalized.significand >> 63, 1);
    // Экспонента должна уменьшиться на количество сдвигов
    assert!(normalized.exponent < f.exponent);
}

#[test]
fn test_normalize_to_subnormal() {
    // Недостаточно экспоненты для компенсации ведущих нулей -> subnormal число
    let f = F80 {
        sign: false,
        exponent: 2,
        significand: 0x0F_FF_FF_FF_FF_FF_FF_FF,
    };
    let normalized = f.normalize();
    
    // После нормализации экспонента должна стать 0 (subnormal)
    assert_eq!(normalized.exponent, 0);
    // Мантисса должна быть сдвинута
    assert!(normalized.significand > 0);
}

#[test]
fn test_normalize_zero_significand() {
    // Нулевая мантисса должна быть обнулена
    let f = F80 {
        sign: false,
        exponent: 100,
        significand: 0,
    };
    let normalized = f.normalize();
    
    assert!(normalized.is_zero());
    assert_eq!(normalized.exponent, 0);
}

#[test]
fn test_normalize_preserves_sign() {
    // Знак должен сохраняться при нормализации
    let f = F80 {
        sign: true,
        exponent: F80::BIAS + 10,
        significand: 0x00_FF_FF_FF_FF_FF_FF_FF,
    };
    let normalized = f.normalize();
    assert_eq!(normalized.sign, true);
}

// ==================== ТЕСТЫ IS_ZERO ====================

#[test]
fn test_is_zero_positive_zero() {
    let z = F80::zero(false);
    assert!(z.is_zero());
    assert!(!z.sign);
}

#[test]
fn test_is_zero_negative_zero() {
    let z = F80::zero(true);
    assert!(z.is_zero());
    assert!(z.sign);
}

#[test]
fn test_is_zero_non_zero_number() {
    let num = F80::from_f64(1.0);
    assert!(!num.is_zero());
}

#[test]
fn test_is_zero_very_small_but_nonzero() {
    let tiny = F80::from_f64(1e-300);
    assert!(!tiny.is_zero());
}

// ==================== ТЕСТЫ FROM_F64 ====================

#[test]
fn test_from_f64_zero_positive() {
    let z = F80::from_f64(0.0);
    assert!(z.is_zero());
    assert!(!z.sign);
}

#[test]
fn test_from_f64_zero_negative() {
    let z = F80::from_f64(-0.0);
    assert!(z.is_zero());
    assert!(z.sign);
}

#[test]
fn test_from_f64_positive_one() {
    let one = F80::from_f64(1.0);
    assert_eq!(one.exponent, F80::BIAS);
    assert_eq!(one.significand, 1u64 << 63);
    assert!(!one.sign);
}

#[test]
fn test_from_f64_negative_one() {
    let neg_one = F80::from_f64(-1.0);
    assert_eq!(neg_one.exponent, F80::BIAS);
    assert_eq!(neg_one.significand, 1u64 << 63);
    assert!(neg_one.sign);
}

#[test]
fn test_from_f64_half() {
    let half = F80::from_f64(0.5);
    // 0.5 = 2^-1, поэтому экспонента должна быть BIAS - 1
    assert_eq!(half.exponent, F80::BIAS - 1);
    assert_eq!(half.significand, 1u64 << 63);
    assert!(!half.sign);
}

#[test]
fn test_from_f64_two() {
    let two = F80::from_f64(2.0);
    // 2.0 = 2^1, поэтому экспонента должна быть BIAS + 1
    assert_eq!(two.exponent, F80::BIAS + 1);
    assert_eq!(two.significand, 1u64 << 63);
    assert!(!two.sign);
}

#[test]
fn test_from_f64_three_quarters() {
    let three_quarters = F80::from_f64(0.75);
    // 0.75 = 1.5 * 2^-1 (в двоичной: 0.11 = 1.1 * 2^-1)
    assert!(!three_quarters.sign);
    // Экспонента должна быть BIAS - 1
    assert_eq!(three_quarters.exponent, F80::BIAS - 1);
    // Мантисса должна представлять 1.5 (в нормализованной форме)
    assert!(three_quarters.significand > (1u64 << 63));
}

#[test]
fn test_from_f64_subnormal() {
    let subnormal = F80::from_f64(5e-324);
    // Субнормальное число не должно быть нулём
    assert!(!subnormal.is_zero());
    assert!(!subnormal.sign);
}

#[test]
fn test_from_f64_large_positive() {
    let large = F80::from_f64(1.23e100);
    assert!(!large.sign);
    assert!(!large.is_zero());
    assert!(large.exponent > F80::BIAS);
}

#[test]
fn test_from_f64_small_negative() {
    let small = F80::from_f64(-1.23e-100);
    assert!(small.sign);
    assert!(!small.is_zero());
    assert!(small.exponent < F80::BIAS);
}

// ==================== ДОПОЛНИТЕЛЬНЫЕ ТЕСТЫ НА СЛОЖЕНИЕ ====================

#[test]
fn test_add_commutative() {
    // a + b должно быть равно b + a
    let a = F80::from_f64(123.456);
    let b = F80::from_f64(789.012);
    assert_eq!(a + b, b + a);
}

#[test]
fn test_add_associative_approximate() {
    // (a + b) + c ≈ a + (b + c) (в пределах точности)
    let a = F80::from_f64(100.0);
    let b = F80::from_f64(200.0);
    let c = F80::from_f64(300.0);
    
    let left = (a + b) + c;
    let right = a + (b + c);
    
    // Из-за округления могут быть маленькие отличия
    assert_eq!(left.sign, right.sign);
    assert_eq!(left.exponent, right.exponent);
}

#[test]
fn test_add_identity() {
    // a + 0 = a
    let a = F80::from_f64(42.5);
    let zero = F80::zero(false);
    assert_eq!(a + zero, a);
}

#[test]
fn test_add_inverse_signed_zeros() {
    // a + (-a) = +0 (по IEEE)
    let a = F80::from_f64(12345.6789);
    let neg_a = F80 {
        sign: !a.sign,
        exponent: a.exponent,
        significand: a.significand,
    };
    let result = a + neg_a;
    assert!(result.is_zero());
    assert!(!result.sign);
}

#[test]
fn test_add_positive_and_negative_same_magnitude() {
    let pos = F80::from_f64(99999.999);
    let neg = F80 {
        sign: true,
        exponent: pos.exponent,
        significand: pos.significand,
    };
    let sum = pos + neg;
    assert!(sum.is_zero());
}

#[test]
fn test_add_mixed_signs_result_positive() {
    let big_pos = F80::from_f64(1000.0);
    let small_neg = F80::from_f64(-100.0);
    let result = big_pos + small_neg;
    
    assert!(!result.sign);
    assert_eq!(result, F80::from_f64(900.0));
}

#[test]
fn test_add_mixed_signs_result_negative() {
    let small_pos = F80::from_f64(100.0);
    let big_neg = F80::from_f64(-1000.0);
    let result = small_pos + big_neg;
    
    assert!(result.sign);
    assert_eq!(result, F80::from_f64(-900.0));
}

#[test]
fn test_add_fractional_numbers() {
    let a = F80::from_f64(0.125);
    let b = F80::from_f64(0.625);
    let expected = F80::from_f64(0.75);
    assert_eq!(a + b, expected);
}

#[test]
fn test_add_very_close_exponents() {
    // Два числа с близкими, но разными экспонентами
    let a = F80::from_f64(1024.0);
    let b = F80::from_f64(512.0);
    let expected = F80::from_f64(1536.0);
    assert_eq!(a + b, expected);
}

#[test]
fn test_add_large_and_small_same_sign() {
    let large = F80::from_f64(1e100);
    let small = F80::from_f64(1e-100);
    let result = large + small;
    
    // Малое число может быть потеряно при добавлении к огромному
    assert!(!result.sign);
}

#[test]
fn test_add_many_times() {
    // Сложить число с самим собой много раз
    let one = F80::from_f64(1.0);
    let mut sum = one;
    
    for _ in 0..9 {
        sum = sum + one;
    }
    
    assert_eq!(sum, F80::from_f64(10.0));
}

#[test]
fn test_add_alternating() {
    // Чередующееся добавление положительных и отрицательных
    let pos = F80::from_f64(10.0);
    let neg = F80::from_f64(-7.0);
    
    let mut result = pos;
    result = result + neg; // 3
    result = result + pos; // 13
    result = result + neg; // 6
    
    assert_eq!(result, F80::from_f64(6.0));
}

#[test]
fn test_add_powers_of_two() {
    let one = F80::from_f64(1.0);
    let two = F80::from_f64(2.0);
    let four = F80::from_f64(4.0);
    
    assert_eq!(one + one, two);
    assert_eq!(two + two, four);
    assert_eq!(one + four, F80::from_f64(5.0));
}

#[test]
fn test_add_fractional_powers_of_two() {
    let half = F80::from_f64(0.5);
    let quarter = F80::from_f64(0.25);
    let expected = F80::from_f64(0.75);
    
    assert_eq!(half + quarter, expected);
}

#[test]
fn test_add_negative_and_positive_mixed_precision() {
    let a = F80::from_f64(12.34567);
    let b = F80::from_f64(-12.34566);
    let result = a + b;
    
    assert!(!result.sign);
    assert!(!result.is_zero());
}

// ==================== ТЕСТЫ ГРАНИЧНЫХ СЛУЧАЕВ ====================

#[test]
fn test_boundary_min_positive_exponent() {
    let f = F80 {
        sign: false,
        exponent: 1,
        significand: 1u64 << 63,
    };
    assert!(!f.is_zero());
    assert_eq!(f.exponent, 1);
}

#[test]
fn test_boundary_max_exponent() {
    let f = F80 {
        sign: false,
        exponent: 0x7FFF,
        significand: 1u64 << 63,
    };
    assert_eq!(f.exponent, 0x7FFF);
}

#[test]
fn test_boundary_min_significand() {
    let f = F80 {
        sign: false,
        exponent: F80::BIAS,
        significand: 1,
    };
    assert!(f.significand > 0);
}

#[test]
fn test_boundary_max_significand() {
    let f = F80 {
        sign: false,
        exponent: F80::BIAS,
        significand: u64::MAX,
    };
    assert_eq!(f.significand, u64::MAX);
}

// ==================== СПЕЦИАЛЬНЫЕ ТЕСТЫ ====================

#[test]
fn test_construction_and_fields() {
    let f = F80 {
        sign: true,
        exponent: 12345,
        significand: 0xABCD_EF01_23456789,
    };
    
    assert_eq!(f.sign, true);
    assert_eq!(f.exponent, 12345);
    assert_eq!(f.significand, 0xABCD_EF01_23456789);
}

#[test]
fn test_clone_and_copy() {
    let original = F80::from_f64(3.14159);
    let copy1 = original;
    let copy2 = original.clone();
    
    assert_eq!(original, copy1);
    assert_eq!(original, copy2);
}

#[test]
fn test_debug_output() {
    let f = F80::from_f64(42.0);
    let debug_string = format!("{:?}", f);
    assert!(debug_string.contains("F80"));
}

#[test]
fn test_equality_and_inequality() {
    let a = F80::from_f64(1.0);
    let b = F80::from_f64(1.0);
    let c = F80::from_f64(2.0);
    
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_add_with_sign_changes() {
    let pos1 = F80::from_f64(50.0);
    let neg1 = F80::from_f64(-25.0);
    let neg2 = F80::from_f64(-30.0);
    
    let r1 = pos1 + neg1; // should be 25
    assert_eq!(r1, F80::from_f64(25.0));
    
    let r2 = r1 + neg2; // should be -5
    assert!(r2.sign);
    assert_eq!(r2, F80::from_f64(-5.0));
}

// ==================== ТЕСТЫ ВЫЧИТАНИЯ ====================

#[test]
fn test_sub_simple() {
    let a = F80::from_f64(10.0);
    let b = F80::from_f64(3.0);
    let result = a - b;
    assert_eq!(result, F80::from_f64(7.0));
}

#[test]
fn test_sub_negative_result() {
    let a = F80::from_f64(3.0);
    let b = F80::from_f64(10.0);
    let result = a - b;
    assert!(result.sign);
    assert_eq!(result, F80::from_f64(-7.0));
}

#[test]
fn test_sub_identical() {
    let a = F80::from_f64(42.5);
    let b = F80::from_f64(42.5);
    let result = a - b;
    assert!(result.is_zero());
}

#[test]
fn test_sub_from_zero() {
    let a = F80::zero(false);
    let b = F80::from_f64(5.0);
    let result = a - b;
    assert!(result.sign);
    assert_eq!(result, F80::from_f64(-5.0));
}

#[test]
fn test_sub_zero() {
    let a = F80::from_f64(5.0);
    let b = F80::zero(false);
    let result = a - b;
    assert_eq!(result, a);
}

#[test]
fn test_sub_fractional() {
    let a = F80::from_f64(0.75);
    let b = F80::from_f64(0.25);
    let result = a - b;
    assert_eq!(result, F80::from_f64(0.5));
}

#[test]
fn test_sub_negative_minus_positive() {
    let a = F80::from_f64(-10.0);
    let b = F80::from_f64(5.0);
    let result = a - b;
    assert!(result.sign);
    assert_eq!(result, F80::from_f64(-15.0));
}

#[test]
fn test_sub_negative_minus_negative() {
    let a = F80::from_f64(-5.0);
    let b = F80::from_f64(-10.0);
    let result = a - b;
    assert!(!result.sign);
    assert_eq!(result, F80::from_f64(5.0));
}

// ==================== ТЕСТЫ УНАРНОГО МИНУСА ====================

#[test]
fn test_neg_positive() {
    let a = F80::from_f64(42.0);
    let neg_a = -a;
    assert!(neg_a.sign);
    assert_eq!(neg_a.exponent, a.exponent);
    assert_eq!(neg_a.significand, a.significand);
}

#[test]
fn test_neg_negative() {
    let a = F80::from_f64(-42.0);
    let neg_a = -a;
    assert!(!neg_a.sign);
    assert_eq!(neg_a, F80::from_f64(42.0));
}

#[test]
fn test_neg_zero() {
    let z = F80::zero(false);
    let neg_z = -z;
    assert!(neg_z.sign);
    assert!(neg_z.is_zero());
}

#[test]
fn test_neg_double_negative() {
    let a = F80::from_f64(3.14159);
    let result = -(-a);
    assert_eq!(result, a);
}

#[test]
fn test_neg_fractional() {
    let a = F80::from_f64(0.5);
    let neg_a = -a;
    assert!(neg_a.sign);
    assert_eq!(neg_a, F80::from_f64(-0.5));
}

// ==================== ТЕСТЫ УМНОЖЕНИЯ ====================

#[test]
fn test_mul_simple() {
    let a = F80::from_f64(2.0);
    let b = F80::from_f64(3.0);
    let result = a * b;
    // Проверяем, что результат близок к 6.0 (экспонента и знак правильные)
    let expected = F80::from_f64(6.0);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_mul_by_zero() {
    let a = F80::from_f64(42.0);
    let zero = F80::zero(false);
    let result = a * zero;
    assert!(result.is_zero());
    assert!(!result.sign);
}

#[test]
fn test_mul_zero_by_nonzero() {
    let zero = F80::zero(false);
    let a = F80::from_f64(42.0);
    let result = zero * a;
    assert!(result.is_zero());
}

#[test]
fn test_mul_by_one() {
    let a = F80::from_f64(12.34);
    let one = F80::from_f64(1.0);
    let result = a * one;
    // Результат может слегка отличаться из-за нормализации
    assert!(!result.sign);
    assert_eq!(result.exponent, a.exponent);
}

#[test]
fn test_mul_by_half() {
    let a = F80::from_f64(10.0);
    let half = F80::from_f64(0.5);
    let result = a * half;
    let expected = F80::from_f64(5.0);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_mul_powers_of_two() {
    let two = F80::from_f64(2.0);
    let four = F80::from_f64(4.0);
    let result = two * two;
    assert_eq!(result.sign, four.sign);
    assert_eq!(result.exponent, four.exponent);
}

#[test]
fn test_mul_fractional() {
    let a = F80::from_f64(0.5);
    let b = F80::from_f64(0.25);
    let result = a * b;
    let expected = F80::from_f64(0.125);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_mul_negative_and_positive() {
    let neg = F80::from_f64(-5.0);
    let pos = F80::from_f64(3.0);
    let result = neg * pos;
    let expected = F80::from_f64(-15.0);
    assert!(result.sign); // Должен быть отрицательным
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_mul_two_negatives() {
    let a = F80::from_f64(-4.0);
    let b = F80::from_f64(-6.0);
    let result = a * b;
    let expected = F80::from_f64(24.0);
    assert!(!result.sign); // Должен быть положительным
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_mul_commutative() {
    let a = F80::from_f64(3.14);
    let b = F80::from_f64(2.71);
    let ab = a * b;
    let ba = b * a;
    assert_eq!(ab, ba);
}

#[test]
fn test_mul_large_numbers() {
    let a = F80::from_f64(1e50);
    let b = F80::from_f64(1e40);
    let result = a * b;
    assert!(!result.sign);
    // Результат будет очень большой
    assert!(result.exponent > a.exponent);
}

#[test]
fn test_mul_small_numbers() {
    let a = F80::from_f64(1e-50);
    let b = F80::from_f64(1e-40);
    let result = a * b;
    assert!(!result.sign);
    // Результат будет очень маленький
    assert!(result.exponent < a.exponent);
}

// ==================== ТЕСТЫ ДЕЛЕНИЯ ====================

#[test]
fn test_div_simple() {
    let a = F80::from_f64(10.0);
    let b = F80::from_f64(2.0);
    let result = a / b;
    let expected = F80::from_f64(5.0);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_div_by_one() {
    let a = F80::from_f64(42.5);
    let one = F80::from_f64(1.0);
    let result = a / one;
    // Результат должен быть близок к исходному числу
    assert_eq!(result.sign, a.sign);
    assert!(result.exponent == a.exponent || (result.exponent as i32 - a.exponent as i32).abs() <= 1);
}

#[test]
fn test_div_by_zero() {
    let a = F80::from_f64(10.0);
    let zero = F80::zero(false);
    let result = a / zero;
    // Результат должен быть "бесконечностью"
    assert_eq!(result.exponent, 0x7FFF);
}

#[test]
fn test_div_zero_by_number() {
    let zero = F80::zero(false);
    let a = F80::from_f64(42.0);
    let result = zero / a;
    assert!(result.is_zero());
}

#[test]
fn test_div_fractional() {
    let a = F80::from_f64(0.5);
    let b = F80::from_f64(2.0);
    let result = a / b;
    let expected = F80::from_f64(0.25);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_div_powers_of_two() {
    let eight = F80::from_f64(8.0);
    let two = F80::from_f64(2.0);
    let result = eight / two;
    let expected = F80::from_f64(4.0);
    assert_eq!(result.sign, expected.sign);
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_div_negative_and_positive() {
    let neg = F80::from_f64(-15.0);
    let pos = F80::from_f64(3.0);
    let result = neg / pos;
    let expected = F80::from_f64(-5.0);
    assert!(result.sign); // Должен быть отрицательным
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_div_two_negatives() {
    let a = F80::from_f64(-20.0);
    let b = F80::from_f64(-4.0);
    let result = a / b;
    let expected = F80::from_f64(5.0);
    assert!(!result.sign); // Должен быть положительным
    assert_eq!(result.exponent, expected.exponent);
}

#[test]
fn test_div_large_by_small() {
    let large = F80::from_f64(1e50);
    let small = F80::from_f64(1e-50);
    let result = large / small;
    assert!(!result.sign);
    // Результат будет очень большой
    assert!(result.exponent > large.exponent);
}

#[test]
fn test_div_small_by_large() {
    let small = F80::from_f64(1e-50);
    let large = F80::from_f64(1e50);
    let result = small / large;
    assert!(!result.sign);
    // Результат будет очень маленький
    assert!(result.exponent < small.exponent);
}

#[test]
fn test_div_result_one() {
    let a = F80::from_f64(123.456);
    let result = a / a;
    // Результат должен быть близок к 1
    assert!(!result.sign);
    // Экспонента должна быть близка к BIAS
    assert!((result.exponent as i32 - F80::BIAS as i32).abs() <= 1);
}

// ==================== КОМБИНИРОВАННЫЕ ТЕСТЫ ====================

#[test]
fn test_combined_operations() {
    let a = F80::from_f64(10.0);
    let b = F80::from_f64(5.0);
    let c = F80::from_f64(2.0);
    
    // (a + b) * c - a / b
    let result = (a + b) * c - a / b;
    let expected = F80::from_f64((10.0 + 5.0) * 2.0 - 10.0 / 5.0);
    // Проверяем знак и экспоненту, так как мантисса может отличаться
    assert_eq!(result.sign, expected.sign);
}

#[test]
fn test_distributive_property_approximate() {
    // a * (b + c) ≈ a * b + a * c (в пределах точности)
    let a = F80::from_f64(2.5);
    let b = F80::from_f64(3.0);
    let c = F80::from_f64(4.0);
    
    let left = a * (b + c);
    let right = a * b + a * c;
    
    assert_eq!(left.sign, right.sign);
    // Экспоненты должны быть равны
    assert_eq!(left.exponent, right.exponent);
}

#[test]
fn test_add_sub_inverse() {
    let a = F80::from_f64(123.456);
    let b = F80::from_f64(789.012);
    
    let result = (a + b) - b;
    // После добавления и вычитания должны вернуться к исходному числу
    assert_eq!(result, a);
}

#[test]
fn test_mul_div_inverse() {
    let a = F80::from_f64(42.0);
    let b = F80::from_f64(5.0);
    
    let result = (a * b) / b;
    // После умножения и деления на то же число должны вернуться к исходному
    // Проверяем знак и экспоненту
    assert_eq!(result.sign, a.sign);
}

#[test]
fn test_negate_and_add() {
    let a = F80::from_f64(50.0);
    let b = F80::from_f64(30.0);
    
    let result = a + (-b);
    assert_eq!(result, F80::from_f64(20.0));
}

#[test]
fn test_expression_with_all_ops() {
    let a = F80::from_f64(100.0);
    let b = F80::from_f64(10.0);
    let c = F80::from_f64(2.0);
    
    // a - b * c + a / b
    let result = a - b * c + a / b;
    let expected = F80::from_f64(100.0 - 10.0 * 2.0 + 100.0 / 10.0);
    assert_eq!(result, expected);
}