library IEEE;
use IEEE.STD_LOGIC_1164.ALL;
use IEEE.NUMERIC_STD.ALL;

entity radio_tb is
end radio_tb;

architecture Behavioral of radio_tb is
	signal clock: std_logic := '0';
	
	signal pcie_rx: std_logic_vector(1 downto 0);
	signal pcie_tx: std_logic_vector(1 downto 0);
	signal pcie_refclk: std_logic_vector(1 downto 0);
	signal pcie_wake_n: std_logic;
	signal pcie_perst_n: std_logic;
	signal pcie_tck: std_logic;
	signal pcie_tdi: std_logic;
	signal pcie_tdo: std_logic;
	signal pcie_tms: std_logic;
	signal pcie_present_n: std_logic;
	signal pcie_smbus_clk: std_logic;
	signal pcie_smbus_data: std_logic;
	signal pcie_rext: std_logic; --?
	signal pcie_refret: std_logic; --?
	signal pcie_aux_power: std_logic;
	signal mipi_csi_a_d: std_logic_vector(7 downto 0);
	signal mipi_csi_a_clk: std_logic_vector(1 downto 0);
	signal gpio_a: std_logic_vector(7 downto 0);
	signal i2c_a_scl: std_logic;
	signal i2c_a_sda: std_logic;
	signal ds90_inta: std_logic;
	signal mipi_csi_b_d: std_logic_vector(7 downto 0);
	signal mipi_csi_b_clk: std_logic_vector(1 downto 0);
	signal gpio_b: std_logic_vector(7 downto 0);
	signal i2c_b_scl: std_logic;
	signal i2c_b_sda: std_logic;
	signal ds90_intb: std_logic;
	signal mipi_csi_c_d: std_logic_vector(7 downto 0);
	signal mipi_csi_c_clk: std_logic_vector(1 downto 0);
	signal gpio_c: std_logic_vector(7 downto 0);
	signal i2c_c_scl: std_logic;
	signal i2c_c_sda: std_logic;
	signal ds90_intc: std_logic;
begin
	clock <= not clock after 5ns;

	uut: entity work.radio generic map(sim => '1') port map(
		pcie_rx => pcie_rx,
		pcie_tx => pcie_tx,
		pcie_refclk => pcie_refclk,
		pcie_wake_n => pcie_wake_n,
		pcie_perst_n => pcie_perst_n,
		pcie_tck => pcie_tck,
		pcie_tdi => pcie_tdi,
		pcie_tdo => pcie_tdo,
		pcie_tms => pcie_tms,
		pcie_present_n => pcie_present_n,
		pcie_smbus_clk => pcie_smbus_clk,
		pcie_smbus_data => pcie_smbus_data,
		pcie_rext => pcie_rext,
		pcie_refret => pcie_refret,
		pcie_aux_power => pcie_aux_power,
		mipi_csi_a_d => mipi_csi_a_d,
		mipi_csi_a_clk => mipi_csi_a_clk,
		gpio_a => gpio_a,
		i2c_a_scl => i2c_a_scl,
		i2c_a_sda => i2c_a_sda,
		ds90_inta => ds90_inta,
		mipi_csi_b_d => mipi_csi_b_d,
		mipi_csi_b_clk => mipi_csi_b_clk,
		gpio_b => gpio_b,
		i2c_b_scl => i2c_b_scl,
		i2c_b_sda => i2c_b_sda,
		ds90_intb => ds90_intb,
		mipi_csi_c_d => mipi_csi_c_d,
		mipi_csi_c_clk => mipi_csi_c_clk,
		gpio_c => gpio_c,
		i2c_c_scl => i2c_c_scl,
		i2c_c_sda => i2c_c_sda,
		ds90_intc => ds90_intc,
		clock => clock);

end Behavioral;

