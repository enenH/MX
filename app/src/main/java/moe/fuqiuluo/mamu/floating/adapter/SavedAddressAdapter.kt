package moe.fuqiuluo.mamu.floating.adapter

import android.annotation.SuppressLint
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import moe.fuqiuluo.mamu.R
import moe.fuqiuluo.mamu.databinding.ItemSavedAddressBinding
import moe.fuqiuluo.mamu.floating.data.local.MemoryBackupManager
import moe.fuqiuluo.mamu.floating.data.model.DisplayValueType
import moe.fuqiuluo.mamu.floating.data.model.SavedAddress

class SavedAddressAdapter(
    private val onItemClick: (SavedAddress, Int) -> Unit = { _, _ -> },
    private val onItemLongClick: (SavedAddress, Int) -> Boolean = { _, _ -> false },
    private val onFreezeToggle: (SavedAddress, Boolean) -> Unit = { _, _ -> },
    private val onItemDelete: (SavedAddress) -> Unit = { _ -> },
    private val onSelectionChanged: (Int) -> Unit = {}
) : RecyclerView.Adapter<SavedAddressAdapter.ViewHolder>() {

    private val addresses = mutableListOf<SavedAddress>()

    /**
     * 选择状态用“稳定ID(address)”来追踪，而不是 position。
     * 这样列表排序/插入导致位置变化时，勾选不会错乱。
     */
    private var isAllSelected = false
    private val selectedIds = HashSet<Long>() // isAllSelected=false 时使用
    private val deselectedIds = HashSet<Long>() // isAllSelected=true 时使用

    init {
        setHasStableIds(true)
    }

    private fun stableIdOf(item: SavedAddress): Long = item.address

    /**
     * 按地址从小到大排序（显示为十六进制，但排序按数值）。
     */
    private fun sortAddressesInPlace() {
        addresses.sortBy { it.address }
    }

    private fun isIdSelected(id: Long): Boolean {
        return if (isAllSelected) {
            id !in deselectedIds
        } else {
            id in selectedIds
        }
    }

    private fun getSelectedCount(): Int {
        return if (isAllSelected) {
            addresses.size - deselectedIds.size
        } else {
            selectedIds.size
        }
    }

    private fun toggleSelection(id: Long, selected: Boolean) {
        if (isAllSelected) {
            if (selected) deselectedIds.remove(id) else deselectedIds.add(id)
        } else {
            if (selected) selectedIds.add(id) else selectedIds.remove(id)
        }
    }

    fun setAddresses(newAddresses: List<SavedAddress>) {
        val oldSize = addresses.size
        addresses.clear()

        // 重置选择状态
        isAllSelected = false
        selectedIds.clear()
        deselectedIds.clear()

        if (oldSize > 0) {
            notifyItemRangeRemoved(0, oldSize)
        }

        addresses.addAll(newAddresses)
        sortAddressesInPlace()

        if (addresses.isNotEmpty()) {
            notifyItemRangeInserted(0, addresses.size)
        }
        onSelectionChanged(0)
    }

    fun addAddress(address: SavedAddress) {
        addresses.add(address)
        sortAddressesInPlace()
        // 排序后插入位置不一定在末尾，直接刷新更安全
        notifyDataSetChanged()
    }

    fun updateAddress(address: SavedAddress) {
        val index = addresses.indexOfFirst { it.address == address.address }
        if (index >= 0) {
            addresses[index] = address
            // 如果更新可能影响排序（地址通常不变，但保持一致性）
            sortAddressesInPlace()
            notifyDataSetChanged()
        }
    }

    fun getSelectedItems(): List<SavedAddress> {
        return if (isAllSelected) {
            addresses.filter { isIdSelected(stableIdOf(it)) }
        } else {
            // 保持返回顺序与列表显示一致
            addresses.filter { stableIdOf(it) in selectedIds }
        }
    }

    fun selectAll() {
        if (isAllSelected && deselectedIds.isEmpty()) return

        isAllSelected = true
        selectedIds.clear()
        deselectedIds.clear()

        notifyItemRangeChanged(0, addresses.size, PAYLOAD_SELECTION_CHANGED)
        onSelectionChanged(addresses.size)
    }

    fun deselectAll() {
        if (!isAllSelected && selectedIds.isEmpty()) return

        isAllSelected = false
        selectedIds.clear()
        deselectedIds.clear()

        notifyItemRangeChanged(0, addresses.size, PAYLOAD_SELECTION_CHANGED)
        onSelectionChanged(0)
    }

    fun invertSelection() {
        isAllSelected = !isAllSelected

        val temp = HashSet(selectedIds)
        selectedIds.clear()
        selectedIds.addAll(deselectedIds)
        deselectedIds.clear()
        deselectedIds.addAll(temp)

        notifyItemRangeChanged(0, addresses.size, PAYLOAD_SELECTION_CHANGED)
        onSelectionChanged(getSelectedCount())
    }

    companion object {
        private const val PAYLOAD_SELECTION_CHANGED = "selection_changed"
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val binding = ItemSavedAddressBinding.inflate(
            LayoutInflater.from(parent.context),
            parent,
            false
        )
        return ViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(addresses[position], position)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int, payloads: MutableList<Any>) {
        if (payloads.isEmpty()) {
            super.onBindViewHolder(holder, position, payloads)
            return
        }

        for (payload in payloads) {
            if (payload == PAYLOAD_SELECTION_CHANGED) {
                holder.updateSelection(addresses[position])
            }
        }
    }

    override fun getItemCount(): Int = addresses.size

    override fun getItemId(position: Int): Long {
        return addresses[position].address
    }

    inner class ViewHolder(
        private val binding: ItemSavedAddressBinding
    ) : RecyclerView.ViewHolder(binding.root) {

        @SuppressLint("SetTextI18n")
        fun bind(address: SavedAddress, position: Int) {
            // checkbox
            val id = stableIdOf(address)
            val isSelected = isIdSelected(id)
            binding.checkbox.setOnCheckedChangeListener(null)
            binding.checkbox.isChecked = isSelected
            updateItemBackground(isSelected)
            binding.checkbox.setOnCheckedChangeListener { _, isChecked ->
                bindingAdapterPosition.takeIf { it != RecyclerView.NO_POSITION }?.let { pos ->
                    val item = addresses[pos]
                    toggleSelection(stableIdOf(item), isChecked)
                    updateItemBackground(isChecked)
                    onSelectionChanged(getSelectedCount())
                }
            }

            // 变量名称
            binding.nameText.text = address.name

            // 地址（大写，无0x前缀）
            binding.addressText.text = String.format("%X", address.address)

            // 值
            binding.valueText.text = address.value.ifBlank { "空空如也" }

            // 备份值（旧值）
            val backup = MemoryBackupManager.getBackup(address.address)
            if (backup != null) {
                binding.backupValueText.text = "(${backup.originalValue})"
                binding.backupValueText.visibility = View.VISIBLE
            } else {
                binding.backupValueText.visibility = View.GONE
            }

            // 数据类型和范围
            val valueType = address.displayValueType ?: DisplayValueType.DWORD
            binding.typeText.text = valueType.code
            binding.typeText.setTextColor(valueType.textColor)
            binding.rangeText.text = address.range.code
            binding.rangeText.setTextColor(address.range.color)

            // 冻结按钮
            binding.freezeButton.apply {
                if (address.isFrozen) {
                    setIconResource(R.drawable.icon_play_arrow_24px)
                } else {
                    setIconResource(R.drawable.icon_pause_24px)
                }

                setOnClickListener {
                    val newFrozenState = !address.isFrozen
                    address.isFrozen = newFrozenState
                    if (newFrozenState) {
                        setIconResource(R.drawable.icon_play_arrow_24px)
                    } else {
                        setIconResource(R.drawable.icon_pause_24px)
                    }
                    onFreezeToggle(address, newFrozenState)
                }
            }

            // 删除按钮
            binding.deleteButton.setOnClickListener { onItemDelete(address) }

            // 点击/长按（position 用于回调显示层逻辑，数据以 address 为准）
            binding.itemContainer.setOnClickListener { onItemClick(address, position) }

            binding.itemContainer.setOnLongClickListener {
                bindingAdapterPosition.takeIf { it != RecyclerView.NO_POSITION }?.let { pos ->
                    onItemLongClick(addresses[pos], pos)
                } ?: false
            }
        }

        fun updateSelection(address: SavedAddress) {
            val isSelected = isIdSelected(stableIdOf(address))
            binding.checkbox.apply {
                setOnCheckedChangeListener(null)
                isChecked = isSelected
                updateItemBackground(isSelected)
                setOnCheckedChangeListener { _, isChecked ->
                    bindingAdapterPosition.takeIf { it != RecyclerView.NO_POSITION }?.let { pos ->
                        val item = addresses[pos]
                        toggleSelection(stableIdOf(item), isChecked)
                        updateItemBackground(isChecked)
                        onSelectionChanged(getSelectedCount())
                    }
                }
            }
        }

        private fun updateItemBackground(isSelected: Boolean) {
            binding.itemContainer.setBackgroundColor(
                if (isSelected) 0x33448AFF else 0x00000000
            )
        }
    }
}
