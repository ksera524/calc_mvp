mod slack;

use std::error::Error;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use slack::send_slack_message;
use dotenv::dotenv;


struct StockRecord {
    stock_symbol: String,
    prices: [Decimal; 15],
    volumes: [i64; 15],
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{
    let pool = establish_connection().await?;

    // データベースから株価データを取得
    let stock_records = fetch_stock_records(&pool).await?;

    // MVPの条件を満たす銘柄を抽出
    let mvp_stocks: Vec<&StockRecord> = stock_records.iter().filter(|rec| check_mvp(rec)).collect();

    if mvp_stocks.is_empty() {
        println!("MVP条件を満たす銘柄はありませんでした");
        send_slack_message("MVP条件を満たす銘柄はありませんでした").await?;
    } else {
        let mut message = "MVP条件を満たす銘柄: ".to_string();
        for rec in mvp_stocks {
            message.push_str(&rec.stock_symbol);
            message.push_str(", ");
        }
        message.pop(); // 最後のカンマを削除
        message.pop(); // 最後のスペースを削除
        send_slack_message(&message).await?;
    }

    Ok(())
}

async fn fetch_stock_records(pool: &PgPool) -> Result<Vec<StockRecord>, sqlx::Error> {
    let records = sqlx::query!(
        "SELECT stock_symbol, array_agg(price ORDER BY date DESC) as prices, array_agg(volume ORDER BY date DESC) as volumes
        FROM (
            SELECT *,
            ROW_NUMBER() OVER (PARTITION BY stock_symbol ORDER BY date DESC) AS rn
            FROM stock_prices
        ) ranked_prices
        WHERE rn <= 15
        GROUP BY stock_symbol"
    )
    .fetch_all(pool)
    .await?;

    let mut stock_records: Vec<StockRecord> = Vec::new();
    for rec in records {
        let prices: Vec<Decimal> = rec.prices.unwrap_or_default();
        let volumes: Vec<i64> = rec.volumes.unwrap_or_default();        

        let mut prices_array: [Decimal; 15] = [dec!(0.0); 15];
        let mut volumes_array: [i64; 15] = [0; 15];
        for (i, price) in prices.into_iter().enumerate().take(15) {
            prices_array[i] = price;
        }
        for (i, volume) in volumes.into_iter().enumerate().take(15) {
            volumes_array[i] = volume;
        }

        stock_records.push(StockRecord {
            stock_symbol: rec.stock_symbol,
            prices: prices_array,
            volumes: volumes_array,
        });
    }

    Ok(stock_records)
}

// ここでMVPの条件を満たしているか判定する
//●Ｍ　モメンタム　15日のうち12日で上げる。
//●Ｖ　出来高　その15日間に出来高が25％以上増える。
//●Ｐ　株価　その15日間に20％以上の上
fn check_mvp(stock_record: &StockRecord) -> bool {
    let mut up_days = 0;
    for i in 0..14 {
        if stock_record.prices[i] > stock_record.prices[i+1] {
            up_days += 1;
        }
    }
    let up_ratio = up_days as f64 / 14.0;

    // 出来高の増加率の分母が0でないことを確認
    if stock_record.volumes[14] == 0 {
        println!("Ticker: {} has 0 volume on the 15th day", stock_record.stock_symbol);
        return false; 
    }
    let volume_increase = Decimal::from(stock_record.volumes[0]) - Decimal::from(stock_record.volumes[14]);
    let volume_ratio = volume_increase / Decimal::from(stock_record.volumes[14]);

    // 株価の増加率の分母が0でないことを確認
    if stock_record.prices[14].is_zero() {
        println!("Ticker: {} has 0 price on the 15th day", stock_record.stock_symbol);
        return false; // 株価の最初の日が0ならfalseを返す
    }
    let price_increase = stock_record.prices[0] - stock_record.prices[14];
    let price_ratio = price_increase / stock_record.prices[14];

    let m_result = up_ratio > 0.8; //15日のうち12日で上げる
    let v_result = volume_ratio >= dec!(0.25);//出来高が25％以上増える
    let p_result = price_ratio >= dec!(0.20);//株価が20％以上の上

    println!("条件1: {} 条件2: {} 条件3: {}", m_result, v_result, p_result);

    // Decimalとf64の比較に注意が必要。適宜、Decimal型の値を使用する
    m_result && v_result && p_result
}

async fn establish_connection() ->  Result<sqlx::Pool<sqlx::Postgres>, sqlx::Error> {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url).await
}


#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn 条件を満たす場合に真を返す() {
        let stock_record = StockRecord {
            stock_symbol: "TEST".to_string(),
            prices: [
                dec!(134.0), dec!(132.0), dec!(131.0), dec!(129.0), dec!(128.0),
                dec!(127.0), dec!(126.0), dec!(124.0), dec!(123.0), dec!(122.0),
                dec!(121.0), dec!(119.0), dec!(117.0), dec!(115.0), dec!(110.0),
            ],
            volumes: [
                2900, 2800, 2700, 2600, 2500,
                2400, 2300, 2200, 2100, 2000,
                1900, 1800, 1700, 1600, 1500,
            ],
        };
        assert!(check_mvp(&stock_record));
    }
    #[test]
    fn 条件を満たさない場合に偽を返す() {
        let stock_record = StockRecord {
            stock_symbol: "FAIL".to_string(),
            prices: [dec!(100.0); 15],
            volumes: [1000; 15],
        };
        assert!(!check_mvp(&stock_record));
    }

    #[test]
    fn 出来高の初日が0で偽を返す() {
        let stock_record = StockRecord {
            stock_symbol: "ZEROVOL".to_string(),
            prices: [dec!(100.0); 15],
            volumes: [0; 15], // 出来高がすべて0
        };
        assert!(!check_mvp(&stock_record));
    }

    #[test]
    fn 株価の初日が0で偽を返す() {
        let stock_record = StockRecord {
            stock_symbol: "ZEROPRICE".to_string(),
            prices: [dec!(0.0); 15], // 株価がすべて0
            volumes: [1000; 15],
        };
        assert!(!check_mvp(&stock_record));
    }

    #[test]
    fn 境界値で真を返す() {
        let stock_record = StockRecord {
            stock_symbol: "BORDERLINE_TRUE".to_string(),
            prices: [
                dec!(120.0), dec!(110.0), // 20%増
                dec!(109.0), dec!(108.0), dec!(107.0), dec!(106.0), dec!(105.0),
                dec!(104.0), dec!(103.0), dec!(102.0), dec!(101.0), dec!(100.0),
                dec!(99.0), dec!(98.0), dec!(90.0),
            ],
            volumes: [
                1250, 1000, 1000, 1000, 1000,
                1000, 1000, 1000, 1000, 1000,
                1000, 1000, 1000, 1000, 1000,
            ], // 25%増
        };
        assert!(check_mvp(&stock_record));
    }

    #[test]
    fn 境界値で偽を返す() {
        let stock_record = StockRecord {
            stock_symbol: "BORDERLINE_FALSE".to_string(),
            prices: [
                dec!(119.9), dec!(110.0), // 20%未満の増加
                dec!(109.0), dec!(108.0), dec!(107.0), dec!(106.0), dec!(105.0),
                dec!(104.0), dec!(103.0), dec!(102.0), dec!(101.0), dec!(100.0),
                dec!(99.0), dec!(98.0), dec!(90.0),
            ],
            volumes: [
                1249, 1000, 1000, 1000, 1000,
                1000, 1000, 1000, 1000, 1000,
                1000, 1000, 1000, 1000, 1000,
            ], // 25%未満の増加
        };
        assert!(!check_mvp(&stock_record));
    }
}
