package moe.fuqiuluo.mamu.driver


/**
 * 搜索模式枚举
 * 对应 Rust 层的 SearchResultMode
 */
enum class SearchMode(val nativeValue: Int) {
    /**
     * 精确搜索（包含联合搜索/范围搜索）
     */
    EXACT(0),

    /**
     * 模糊搜索
     */
    FUZZY(1);

    companion object {
        fun fromNativeValue(value: Int): SearchMode {
            return entries.firstOrNull { it.nativeValue == value } ?: EXACT
        }
    }
}